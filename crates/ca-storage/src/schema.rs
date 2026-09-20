use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::StorageError;

pub const CURRENT_SCHEMA_VERSION: u32 = 6;
pub(crate) const SQLITE_APPLICATION_ID: u32 = 0x4341_544c;

const MIGRATION_1: &str = r#"
CREATE TABLE generations (
    id TEXT PRIMARY KEY,
    parent_id TEXT REFERENCES generations(id),
    status TEXT NOT NULL CHECK (status IN ('building', 'active', 'superseded', 'abandoned')),
    scan_complete INTEGER NOT NULL CHECK (scan_complete IN (0, 1)),
    created_at INTEGER NOT NULL,
    activated_at INTEGER
) STRICT;

CREATE TABLE meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    schema_version INTEGER NOT NULL,
    root_id TEXT NOT NULL,
    active_generation_id TEXT REFERENCES generations(id),
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    relative_path TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE file_versions (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    content_hash TEXT NOT NULL,
    extractor_hash TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    source_encoding TEXT NOT NULL CHECK (source_encoding = 'utf-8'),
    UNIQUE(file_id, content_hash, extractor_hash)
) STRICT;

CREATE TABLE generation_files (
    generation_id TEXT NOT NULL REFERENCES generations(id) ON DELETE CASCADE,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE RESTRICT,
    PRIMARY KEY(generation_id, file_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE facts (
    id TEXT NOT NULL,
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    PRIMARY KEY(file_version_id, id)
) STRICT, WITHOUT ROWID;

CREATE INDEX facts_name_idx ON facts(name);

CREATE TABLE search_documents (
    id INTEGER PRIMARY KEY,
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    content TEXT NOT NULL,
    UNIQUE(file_version_id, name, content)
) STRICT;

CREATE VIRTUAL TABLE symbol_fts USING fts5(
    name,
    path,
    content,
    content='search_documents',
    content_rowid='id'
);

CREATE TRIGGER search_documents_ai AFTER INSERT ON search_documents BEGIN
    INSERT INTO symbol_fts(rowid, name, path, content)
    VALUES (new.id, new.name, new.path, new.content);
END;
CREATE TRIGGER search_documents_ad AFTER DELETE ON search_documents BEGIN
    INSERT INTO symbol_fts(symbol_fts, rowid, name, path, content)
    VALUES ('delete', old.id, old.name, old.path, old.content);
END;
CREATE TRIGGER search_documents_au AFTER UPDATE ON search_documents BEGIN
    INSERT INTO symbol_fts(symbol_fts, rowid, name, path, content)
    VALUES ('delete', old.id, old.name, old.path, old.content);
    INSERT INTO symbol_fts(rowid, name, path, content)
    VALUES (new.id, new.name, new.path, new.content);
END;

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    generation_id TEXT REFERENCES generations(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'staged', 'completed', 'failed', 'interrupted')),
    owner_instance_id TEXT NOT NULL,
    error_summary TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;
"#;

const MIGRATION_2: &str = r#"
ALTER TABLE memories ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
UPDATE memories SET updated_at = created_at WHERE updated_at = 0;

CREATE TABLE memory_evidence (
    memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    repository_id TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    PRIMARY KEY(memory_id, repository_id, relative_path, content_hash)
) STRICT, WITHOUT ROWID;

CREATE VIRTUAL TABLE memory_fts USING fts5(
    body,
    content='memories',
    content_rowid='rowid'
);
INSERT INTO memory_fts(rowid, body) SELECT rowid, body FROM memories;
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memory_fts(rowid, body) VALUES (new.rowid, new.body);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, body) VALUES ('delete', old.rowid, old.body);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, body) VALUES ('delete', old.rowid, old.body);
    INSERT INTO memory_fts(rowid, body) VALUES (new.rowid, new.body);
END;
"#;

const MIGRATION_3: &str = r#"
ALTER TABLE generations ADD COLUMN coverage_status TEXT NOT NULL DEFAULT 'syntax_only';
ALTER TABLE generations ADD COLUMN warning_count INTEGER NOT NULL DEFAULT 0 CHECK (warning_count >= 0);
ALTER TABLE generations ADD COLUMN failure_summary TEXT;
ALTER TABLE generations ADD COLUMN config_hash TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE generations ADD COLUMN extractor_set_hash TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE generations ADD COLUMN completed_at INTEGER;

ALTER TABLE file_versions ADD COLUMN language TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE file_versions ADD COLUMN grammar_hash TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE file_versions ADD COLUMN query_hash TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE file_versions ADD COLUMN parse_status TEXT NOT NULL DEFAULT 'complete'
    CHECK (parse_status IN ('complete', 'partial', 'failed'));
ALTER TABLE file_versions ADD COLUMN coverage_json TEXT NOT NULL DEFAULT '{}';

ALTER TABLE jobs RENAME TO jobs_v2;
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    generation_id TEXT REFERENCES generations(id) ON DELETE SET NULL,
    operation TEXT NOT NULL CHECK (operation = 'index'),
    mode TEXT NOT NULL CHECK (mode IN ('incremental', 'full')),
    request_key TEXT,
    status TEXT NOT NULL CHECK (status IN (
        'queued', 'scanning', 'parsing', 'resolving', 'committing',
        'completed', 'cancelled', 'failed', 'interrupted'
    )),
    owner_instance_id TEXT NOT NULL,
    cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
    files_discovered INTEGER NOT NULL DEFAULT 0 CHECK (files_discovered >= 0),
    files_reused INTEGER NOT NULL DEFAULT 0 CHECK (files_reused >= 0),
    files_parsed INTEGER NOT NULL DEFAULT 0 CHECK (files_parsed >= 0),
    files_failed INTEGER NOT NULL DEFAULT 0 CHECK (files_failed >= 0),
    files_persisted INTEGER NOT NULL DEFAULT 0 CHECK (files_persisted >= 0),
    files_deleted INTEGER NOT NULL DEFAULT 0 CHECK (files_deleted >= 0),
    warning_count INTEGER NOT NULL DEFAULT 0 CHECK (warning_count >= 0),
    error_summary TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
INSERT INTO jobs(
    id, generation_id, operation, mode, status, owner_instance_id,
    error_summary, created_at, updated_at
)
SELECT id, generation_id, 'index', 'full',
       CASE status
           WHEN 'running' THEN 'parsing'
           WHEN 'staged' THEN 'committing'
           ELSE status
       END,
       owner_instance_id, error_summary, created_at, updated_at
FROM jobs_v2;
DROP TABLE jobs_v2;
CREATE UNIQUE INDEX jobs_request_key_idx
    ON jobs(operation, request_key) WHERE request_key IS NOT NULL;
CREATE INDEX jobs_status_idx ON jobs(status, updated_at);

CREATE TABLE symbols (
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    observation_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    spelling TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    syntax_start_byte INTEGER NOT NULL CHECK (syntax_start_byte >= 0),
    syntax_end_byte INTEGER NOT NULL CHECK (syntax_end_byte >= syntax_start_byte),
    scope_id TEXT,
    container TEXT,
    signature TEXT,
    receiver TEXT,
    alias TEXT,
    target_id TEXT,
    resolution TEXT NOT NULL,
    attributes_json TEXT NOT NULL,
    limitations_json TEXT NOT NULL,
    PRIMARY KEY(file_version_id, observation_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE scopes (
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    observation_id TEXT NOT NULL, kind TEXT NOT NULL, spelling TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    syntax_start_byte INTEGER NOT NULL CHECK (syntax_start_byte >= 0),
    syntax_end_byte INTEGER NOT NULL CHECK (syntax_end_byte >= syntax_start_byte),
    scope_id TEXT, container TEXT, signature TEXT, receiver TEXT, alias TEXT, target_id TEXT,
    resolution TEXT NOT NULL, attributes_json TEXT NOT NULL, limitations_json TEXT NOT NULL,
    PRIMARY KEY(file_version_id, observation_id)
) STRICT, WITHOUT ROWID;
CREATE TABLE imports (
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    observation_id TEXT NOT NULL, kind TEXT NOT NULL, spelling TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    syntax_start_byte INTEGER NOT NULL CHECK (syntax_start_byte >= 0),
    syntax_end_byte INTEGER NOT NULL CHECK (syntax_end_byte >= syntax_start_byte),
    scope_id TEXT, container TEXT, signature TEXT, receiver TEXT, alias TEXT, target_id TEXT,
    resolution TEXT NOT NULL, attributes_json TEXT NOT NULL, limitations_json TEXT NOT NULL,
    PRIMARY KEY(file_version_id, observation_id)
) STRICT, WITHOUT ROWID;
CREATE TABLE "references" (
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    observation_id TEXT NOT NULL, kind TEXT NOT NULL, spelling TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    syntax_start_byte INTEGER NOT NULL CHECK (syntax_start_byte >= 0),
    syntax_end_byte INTEGER NOT NULL CHECK (syntax_end_byte >= syntax_start_byte),
    scope_id TEXT, container TEXT, signature TEXT, receiver TEXT, alias TEXT, target_id TEXT,
    resolution TEXT NOT NULL, attributes_json TEXT NOT NULL, limitations_json TEXT NOT NULL,
    PRIMARY KEY(file_version_id, observation_id)
) STRICT, WITHOUT ROWID;
CREATE TABLE call_sites (
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    observation_id TEXT NOT NULL, kind TEXT NOT NULL, spelling TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    syntax_start_byte INTEGER NOT NULL CHECK (syntax_start_byte >= 0),
    syntax_end_byte INTEGER NOT NULL CHECK (syntax_end_byte >= syntax_start_byte),
    scope_id TEXT, container TEXT, signature TEXT, receiver TEXT, alias TEXT, target_id TEXT,
    resolution TEXT NOT NULL, attributes_json TEXT NOT NULL, limitations_json TEXT NOT NULL,
    PRIMARY KEY(file_version_id, observation_id)
) STRICT, WITHOUT ROWID;
CREATE TABLE conditions (
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    observation_id TEXT NOT NULL, kind TEXT NOT NULL, spelling TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    syntax_start_byte INTEGER NOT NULL CHECK (syntax_start_byte >= 0),
    syntax_end_byte INTEGER NOT NULL CHECK (syntax_end_byte >= syntax_start_byte),
    scope_id TEXT, container TEXT, signature TEXT, receiver TEXT, alias TEXT, target_id TEXT,
    resolution TEXT NOT NULL, attributes_json TEXT NOT NULL, limitations_json TEXT NOT NULL,
    PRIMARY KEY(file_version_id, observation_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX symbols_name_idx ON symbols(spelling, kind);
CREATE INDEX symbols_scope_idx ON symbols(scope_id);
CREATE INDEX scopes_range_idx ON scopes(file_version_id, start_byte, end_byte);
CREATE INDEX imports_name_idx ON imports(spelling);
CREATE INDEX references_name_idx ON "references"(spelling);
CREATE INDEX call_sites_name_idx ON call_sites(spelling);

CREATE TABLE diagnostics (
    file_version_id INTEGER NOT NULL REFERENCES file_versions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('information', 'warning', 'error')),
    start_byte INTEGER,
    end_byte INTEGER,
    PRIMARY KEY(file_version_id, ordinal)
) STRICT, WITHOUT ROWID;
"#;

const MIGRATION_4: &str = r#"
ALTER TABLE generations ADD COLUMN resolver_version TEXT NOT NULL DEFAULT 'none';
ALTER TABLE generations ADD COLUMN resolved_edge_count INTEGER NOT NULL DEFAULT 0
    CHECK (resolved_edge_count >= 0);
ALTER TABLE generations ADD COLUMN unresolved_occurrence_count INTEGER NOT NULL DEFAULT 0
    CHECK (unresolved_occurrence_count >= 0);

CREATE UNIQUE INDEX generation_files_generation_version_idx
    ON generation_files(generation_id, file_version_id);

CREATE TABLE external_nodes (
    generation_id TEXT NOT NULL REFERENCES generations(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL,
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY(generation_id, node_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE resolved_edges (
    generation_id TEXT NOT NULL REFERENCES generations(id) ON DELETE CASCADE,
    edge_id TEXT NOT NULL,
    relationship TEXT NOT NULL CHECK (relationship IN (
        'contains', 'imports', 'references', 'calls'
    )),
    source_file_version_id INTEGER NOT NULL,
    source_observation_id TEXT NOT NULL,
    source_category TEXT NOT NULL CHECK (source_category IN (
        'symbol', 'scope', 'import', 'reference', 'call_site'
    )),
    source_start_byte INTEGER NOT NULL CHECK (source_start_byte >= 0),
    source_end_byte INTEGER NOT NULL CHECK (source_end_byte >= source_start_byte),
    source_symbol_id TEXT,
    target_file_version_id INTEGER,
    target_symbol_id TEXT,
    external_node_id TEXT,
    resolution TEXT NOT NULL CHECK (resolution IN (
        'syntax_observation', 'lexically_resolved', 'candidate', 'unresolved'
    )),
    rule_version TEXT NOT NULL,
    resolver_version TEXT NOT NULL,
    candidate_count INTEGER NOT NULL CHECK (candidate_count >= 0),
    reason TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    limitations_json TEXT NOT NULL,
    PRIMARY KEY(generation_id, edge_id),
    FOREIGN KEY(generation_id, source_file_version_id)
        REFERENCES generation_files(generation_id, file_version_id) ON DELETE CASCADE,
    FOREIGN KEY(source_file_version_id, source_symbol_id)
        REFERENCES symbols(file_version_id, observation_id) ON DELETE CASCADE,
    FOREIGN KEY(generation_id, target_file_version_id)
        REFERENCES generation_files(generation_id, file_version_id) ON DELETE CASCADE,
    FOREIGN KEY(target_file_version_id, target_symbol_id)
        REFERENCES symbols(file_version_id, observation_id) ON DELETE CASCADE,
    FOREIGN KEY(generation_id, external_node_id)
        REFERENCES external_nodes(generation_id, node_id) ON DELETE CASCADE,
    CHECK (target_symbol_id IS NULL OR target_file_version_id IS NOT NULL),
    CHECK (NOT (target_file_version_id IS NOT NULL AND external_node_id IS NOT NULL)),
    CHECK (
        resolution <> 'unresolved'
        OR (target_file_version_id IS NULL AND external_node_id IS NULL)
    )
) STRICT, WITHOUT ROWID;

CREATE INDEX resolved_edges_outgoing_idx
    ON resolved_edges(generation_id, source_symbol_id, relationship, resolution, edge_id);
CREATE INDEX resolved_edges_incoming_idx
    ON resolved_edges(generation_id, target_symbol_id, relationship, resolution, edge_id);
CREATE INDEX resolved_edges_source_observation_idx
    ON resolved_edges(generation_id, source_observation_id, relationship, resolution, edge_id);

CREATE TABLE resolution_diagnostics (
    generation_id TEXT NOT NULL REFERENCES generations(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    file_version_id INTEGER NOT NULL,
    observation_id TEXT,
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    PRIMARY KEY(generation_id, ordinal),
    FOREIGN KEY(generation_id, file_version_id)
        REFERENCES generation_files(generation_id, file_version_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE INDEX resolution_diagnostics_file_idx
    ON resolution_diagnostics(generation_id, file_version_id, code);
"#;

const MIGRATION_5: &str = r#"
ALTER TABLE memories ADD COLUMN kind TEXT NOT NULL DEFAULT 'convention'
    CHECK (kind IN ('decision', 'convention', 'pitfall', 'task'));
ALTER TABLE memories ADD COLUMN revision INTEGER NOT NULL DEFAULT 1
    CHECK (revision >= 1);
ALTER TABLE memories ADD COLUMN author TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE memories ADD COLUMN origin TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'repository';
ALTER TABLE memory_evidence ADD COLUMN symbol_id TEXT;

CREATE INDEX memories_scope_updated_idx ON memories(scope, updated_at DESC, id);
CREATE INDEX memory_evidence_path_idx
    ON memory_evidence(repository_id, relative_path, memory_id);
CREATE INDEX memory_evidence_symbol_idx
    ON memory_evidence(repository_id, symbol_id, memory_id)
    WHERE symbol_id IS NOT NULL;
"#;

const MIGRATION_6: &str = r#"
CREATE INDEX generation_files_file_version_idx
    ON generation_files(file_version_id);
CREATE INDEX symbols_folded_spelling_idx
    ON symbols(lower(spelling));
"#;

pub(crate) fn migrate(connection: &mut Connection, root_id: &str) -> Result<(), StorageError> {
    migrate_to(connection, root_id, CURRENT_SCHEMA_VERSION)
}

pub(crate) fn migrate_to(
    connection: &mut Connection,
    root_id: &str,
    target: u32,
) -> Result<(), StorageError> {
    let mut version = preflight(connection, root_id)?;
    while version < target {
        let next = version + 1;
        let transaction = connection.transaction()?;
        match next {
            1 => apply_migration_1(&transaction, root_id)?,
            2 => apply_migration_2(&transaction)?,
            3 => apply_migration_3(&transaction)?,
            4 => apply_migration_4(&transaction)?,
            5 => apply_migration_5(&transaction)?,
            6 => apply_migration_6(&transaction)?,
            _ => {
                return Err(StorageError::FutureSchema {
                    found: next,
                    supported: CURRENT_SCHEMA_VERSION,
                });
            }
        }
        transaction.pragma_update(None, "user_version", next)?;
        transaction.execute(
            "UPDATE meta SET schema_version = ?1 WHERE singleton = 1",
            [next],
        )?;
        transaction.commit()?;
        version = next;
    }
    validate_metadata(connection, root_id, version)
}

pub(crate) fn preflight(connection: &Connection, root_id: &str) -> Result<u32, StorageError> {
    let version = schema_version(connection)?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::FutureSchema {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    if version == 0 && has_user_tables(connection)? {
        return Err(StorageError::UnrecognizedSchema);
    }
    if version > 0 {
        let application_id: u32 =
            connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        if application_id != SQLITE_APPLICATION_ID {
            return Err(StorageError::UnrecognizedSchema);
        }
        validate_metadata(connection, root_id, version)?;
    }
    Ok(version)
}

fn validate_metadata(
    connection: &Connection,
    root_id: &str,
    version: u32,
) -> Result<(), StorageError> {
    let stored_root: String =
        connection.query_row("SELECT root_id FROM meta WHERE singleton = 1", [], |row| {
            row.get(0)
        })?;
    let recorded_version: u32 = connection.query_row(
        "SELECT schema_version FROM meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if recorded_version != version {
        return Err(StorageError::SchemaMetadataMismatch);
    }
    if stored_root != root_id {
        return Err(StorageError::RootIdentityMismatch {
            expected: root_id.to_owned(),
            found: stored_root,
        });
    }
    Ok(())
}

fn apply_migration_1(transaction: &Transaction<'_>, root_id: &str) -> Result<(), StorageError> {
    transaction.execute_batch(MIGRATION_1)?;
    transaction.execute(
        "INSERT INTO meta(singleton, schema_version, root_id, active_generation_id, updated_at)
         VALUES (1, 1, ?1, NULL, 0)",
        [root_id],
    )?;
    transaction.pragma_update(None, "application_id", SQLITE_APPLICATION_ID)?;
    Ok(())
}

fn apply_migration_2(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(MIGRATION_2)?;
    Ok(())
}

fn apply_migration_3(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(MIGRATION_3)?;
    Ok(())
}

fn apply_migration_4(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(MIGRATION_4)?;
    Ok(())
}

fn apply_migration_5(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(MIGRATION_5)?;
    Ok(())
}

fn apply_migration_6(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(MIGRATION_6)?;
    Ok(())
}

pub(crate) fn schema_version(connection: &Connection) -> Result<u32, StorageError> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(StorageError::from)
}

fn has_user_tables(connection: &Connection) -> Result<bool, StorageError> {
    let table: Option<String> = connection
        .query_row(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(table.is_some())
}
