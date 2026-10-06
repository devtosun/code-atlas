use std::{
    collections::{BTreeMap, HashSet},
    env,
    fs::{self, File, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ca_core::{ByteRange, GenerationId, JobId, RepositoryId};
use fs4::TryLockError;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, backup::Backup, params};
use thiserror::Error;

use crate::{CURRENT_SCHEMA_VERSION, schema};

const MIN_SQLITE_VERSION: (u32, u32, u32) = (3, 51, 3);
const BUSY_TIMEOUT: Duration = Duration::from_millis(250);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const WRITER_QUEUE_CAPACITY: usize = 16;
const MAX_STAGE_FILES: usize = 1_000;
const MAX_FACTS_PER_FILE: usize = 10_000;
const MAX_SEARCH_DOCUMENTS_PER_FILE: usize = 10_000;
const MAX_MEMORY_BYTES: usize = 16 * 1024;
const MAX_MEMORIES: usize = 10_000;
const MAX_MEMORY_EVIDENCE: usize = 32;
const MAX_MEMORY_EVIDENCE_BYTES: usize = 8 * 1024;
const MAX_MEMORY_METADATA_BYTES: usize = 256;
const MAX_SEARCH_QUERY_BYTES: usize = 256;
const MAX_REUSE_FILES_PER_BATCH: usize = 1_000;
const MAX_JOB_ERROR_BYTES: usize = 4_096;
const MAX_REQUEST_KEY_BYTES: usize = 256;
const MAX_RESOLUTION_EDGES: usize = 1_000_000;
const MAX_EXTERNAL_NODES: usize = 100_000;
const MAX_RESOLUTION_DIAGNOSTICS: usize = 100_000;

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_MEMORY_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("cannot access storage path '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("write owner is busy; retry after {retry_after_ms} ms")]
    WriterBusy { retry_after_ms: u64 },
    #[error("writer queue is full; retry the bounded mutation")]
    QueueFull,
    #[error("writer thread disconnected")]
    WriterDisconnected,
    #[error("writer command '{operation}' exceeded the bounded wait")]
    WriterTimeout { operation: &'static str },
    #[error("writer thread panicked")]
    WriterPanicked,
    #[error("SQLite {found} is below the required safety floor 3.51.3")]
    SqliteTooOld { found: String },
    #[error("the linked SQLite runtime does not provide FTS5")]
    Fts5Unavailable,
    #[error("SQLite journal mode is '{found}', expected WAL")]
    WalUnavailable { found: String },
    #[error("database schema {found} is newer than supported schema {supported}")]
    FutureSchema { found: u32, supported: u32 },
    #[error("database schema {found} requires the write owner to migrate to {required}")]
    MigrationRequired { found: u32, required: u32 },
    #[error("database has tables but no recognized CodeAtlas schema")]
    UnrecognizedSchema,
    #[error("database schema metadata is inconsistent")]
    SchemaMetadataMismatch,
    #[error("database belongs to repository '{found}', expected '{expected}'")]
    RootIdentityMismatch { expected: String, found: String },
    #[error("no active generation exists")]
    IndexNotReady,
    #[error("generation '{generation}' cannot be activated: {reason}")]
    InvalidGeneration {
        generation: String,
        reason: &'static str,
    },
    #[error("job '{0}' does not exist")]
    JobNotFound(String),
    #[error("job '{job}' cannot transition from {from} to {to}")]
    InvalidJobTransition {
        job: String,
        from: String,
        to: String,
    },
    #[error("memory '{0}' does not exist")]
    MemoryNotFound(String),
    #[error("memory '{id}' requires expected_revision for an update")]
    MemoryRevisionRequired { id: String },
    #[error("memory '{id}' revision conflict: expected {expected}, actual {actual}")]
    MemoryRevisionConflict {
        id: String,
        expected: u64,
        actual: u64,
    },
    #[error("memory count limit of {limit} was reached")]
    MemoryLimitExceeded { limit: usize },
    #[error("memory evidence is not valid for the active generation: {0}")]
    MemoryEvidenceInvalid(String),
    #[error("invalid storage input: {0}")]
    InvalidInput(String),
    #[error("application data directory must be absolute: {0}")]
    DataDirectoryNotAbsolute(PathBuf),
    #[error("application data directory must remain outside the authorized source root: {0}")]
    DataDirectoryInsideRoot(PathBuf),
    #[error("no local application-data directory is available on this platform")]
    NoApplicationDataDirectory,
    #[error("backup destination already exists: {0}")]
    BackupExists(PathBuf),
    #[error("storage path is a symbolic link, junction, or other reparse point: {0}")]
    UnsafePath(PathBuf),
}

fn io_error(path: impl Into<PathBuf>, source: io::Error) -> StorageError {
    StorageError::Io {
        path: path.into(),
        source,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePaths {
    data_directory: PathBuf,
    database_path: PathBuf,
    lock_path: PathBuf,
    authorized_root: PathBuf,
}

impl StoragePaths {
    pub fn platform(repository_id: &RepositoryId, root: &Path) -> Result<Self, StorageError> {
        Self::under(platform_data_base()?, repository_id, root)
    }

    pub fn under(
        base: impl Into<PathBuf>,
        repository_id: &RepositoryId,
        root: &Path,
    ) -> Result<Self, StorageError> {
        let base = base.into();
        if !base.is_absolute() {
            return Err(StorageError::DataDirectoryNotAbsolute(base));
        }
        if base
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(StorageError::InvalidInput(
                "application data base cannot contain '.' or '..' components".to_owned(),
            ));
        }
        let authorized_root = fs::canonicalize(root).map_err(|source| io_error(root, source))?;
        let application = if cfg!(target_os = "windows") || cfg!(target_os = "macos") {
            base.join("CodeAtlas")
        } else {
            base.join("codeatlas")
        };
        let requested = application.join("worktrees").join(repository_id.as_str());
        let data_directory = resolve_existing_ancestor(&requested)?;
        if data_directory.starts_with(&authorized_root) {
            return Err(StorageError::DataDirectoryInsideRoot(data_directory));
        }
        Ok(Self {
            database_path: data_directory.join("index.sqlite3"),
            lock_path: data_directory.join("writer.lock"),
            data_directory,
            authorized_root,
        })
    }

    #[must_use]
    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    #[must_use]
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    #[must_use]
    pub const fn permission_policy() -> &'static str {
        if cfg!(unix) {
            "owner-only directory 0700 and files 0600"
        } else {
            "current-user application-data ACL inherited from the platform directory"
        }
    }

    fn prepare(&self) -> Result<(), StorageError> {
        let resolved_before_create = resolve_existing_ancestor(&self.data_directory)?;
        if resolved_before_create != self.data_directory {
            return Err(StorageError::UnsafePath(self.data_directory.clone()));
        }
        if resolved_before_create.starts_with(&self.authorized_root) {
            return Err(StorageError::DataDirectoryInsideRoot(
                resolved_before_create,
            ));
        }
        fs::create_dir_all(&self.data_directory)
            .map_err(|source| io_error(&self.data_directory, source))?;
        let canonical = fs::canonicalize(&self.data_directory)
            .map_err(|source| io_error(&self.data_directory, source))?;
        if canonical != self.data_directory {
            return Err(StorageError::UnsafePath(self.data_directory.clone()));
        }
        if canonical.starts_with(&self.authorized_root) {
            return Err(StorageError::DataDirectoryInsideRoot(canonical));
        }
        set_directory_permissions(&self.data_directory)
    }
}

fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, StorageError> {
    let mut ancestor = path.to_owned();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => {
                let mut resolved =
                    fs::canonicalize(&ancestor).map_err(|source| io_error(&ancestor, source))?;
                if !fs::metadata(&resolved)
                    .map_err(|source| io_error(&resolved, source))?
                    .is_dir()
                {
                    return Err(StorageError::InvalidInput(format!(
                        "application-data ancestor is not a directory: {}",
                        ancestor.display()
                    )));
                }
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(name) = ancestor.file_name() else {
                    return Err(io_error(&ancestor, error));
                };
                missing.push(name.to_os_string());
                if !ancestor.pop() {
                    return Err(io_error(path, error));
                }
            }
            Err(source) => return Err(io_error(&ancestor, source)),
        }
    }
}

fn platform_data_base() -> Result<PathBuf, StorageError> {
    #[cfg(target_os = "windows")]
    {
        return env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(StorageError::NoApplicationDataDirectory);
    }
    #[cfg(target_os = "macos")]
    {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support"))
            .ok_or(StorageError::NoApplicationDataDirectory);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(path));
        }
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".local/share"))
            .ok_or(StorageError::NoApplicationDataDirectory);
    }
    #[allow(unreachable_code)]
    Err(StorageError::NoApplicationDataDirectory)
}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|source| io_error(path, source))
}

#[cfg(not(unix))]
fn set_directory_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|source| io_error(path, source))
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

#[cfg(unix)]
fn set_open_file_permissions(file: &File, path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|source| io_error(path, source))
}

#[cfg(not(unix))]
fn set_open_file_permissions(_file: &File, _path: &Path) -> Result<(), StorageError> {
    Ok(())
}

fn reject_link_like(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link_like(&metadata) => Err(StorageError::UnsafePath(path.to_owned())),
        Ok(metadata) if !metadata.is_file() => Err(StorageError::InvalidInput(format!(
            "storage file path is not a regular file: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error(path, source)),
    }
}

#[cfg(windows)]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteCapabilities {
    pub version: String,
    pub fts5: bool,
    pub journal_mode: String,
    pub schema_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageRole {
    Owner,
    Follower,
}

impl StorageRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Follower => "read_only_follower",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFact {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub byte_range: ByteRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchDocument {
    pub name: String,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationCategory {
    Symbol,
    Scope,
    Import,
    Reference,
    CallSite,
    Condition,
}

impl ObservationCategory {
    const fn table_name(self) -> &'static str {
        match self {
            Self::Symbol => "symbols",
            Self::Scope => "scopes",
            Self::Import => "imports",
            Self::Reference => "\"references\"",
            Self::CallSite => "call_sites",
            Self::Condition => "conditions",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredObservation {
    pub category: ObservationCategory,
    pub id: String,
    pub kind: String,
    pub spelling: String,
    pub byte_range: ByteRange,
    pub syntax_range: ByteRange,
    pub scope_id: Option<String>,
    pub container: Option<String>,
    pub signature: Option<String>,
    pub receiver: Option<String>,
    pub alias: Option<String>,
    pub target_id: Option<String>,
    pub resolution: String,
    pub attributes_json: String,
    pub limitations_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub byte_range: Option<ByteRange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedFile {
    pub relative_path: String,
    pub content_hash: String,
    pub extractor_hash: String,
    pub language: String,
    pub grammar_hash: String,
    pub query_hash: String,
    pub parse_status: String,
    pub coverage_json: String,
    pub byte_length: u64,
    pub facts: Vec<StoredFact>,
    pub observations: Vec<StoredObservation>,
    pub diagnostics: Vec<StoredDiagnostic>,
    pub search_documents: Vec<SearchDocument>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationPlan {
    pub generation_id: GenerationId,
    pub job_id: JobId,
    pub config_hash: String,
    pub extractor_set_hash: String,
    pub reuse_parent: bool,
    pub deleted_paths: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationCompletion {
    pub generation_id: GenerationId,
    pub scan_complete: bool,
    pub coverage_status: String,
    pub warning_count: u64,
    pub failure_summary: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobState {
    Queued,
    Scanning,
    Parsing,
    Resolving,
    Committing,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

impl JobState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Scanning => "scanning",
            Self::Parsing => "parsing",
            Self::Resolving => "resolving",
            Self::Committing => "committing",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    fn from_str(value: &str) -> Result<Self, StorageError> {
        match value {
            "queued" => Ok(Self::Queued),
            "scanning" => Ok(Self::Scanning),
            "parsing" => Ok(Self::Parsing),
            "resolving" => Ok(Self::Resolving),
            "committing" => Ok(Self::Committing),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(StorageError::InvalidInput(format!(
                "unknown stored job state '{value}'"
            ))),
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::Interrupted
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JobProgress {
    pub files_discovered: u64,
    pub files_reused: u64,
    pub files_parsed: u64,
    pub files_failed: u64,
    pub files_persisted: u64,
    pub files_deleted: u64,
    pub warning_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewJob {
    pub mode: String,
    pub request_key: Option<String>,
    pub owner_instance_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobRecord {
    pub id: JobId,
    pub generation_id: Option<GenerationId>,
    pub mode: String,
    pub request_key: Option<String>,
    pub state: JobState,
    pub cancel_requested: bool,
    pub progress: JobProgress,
    pub error_summary: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedJob {
    pub job: JobRecord,
    pub deduplicated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelJobResult {
    Requested,
    AlreadyRequested,
    AlreadyCompleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveFile {
    pub relative_path: String,
    pub content_hash: String,
    pub grammar_hash: String,
    pub query_hash: String,
    pub extractor_hash: String,
    pub config_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredResolutionFile {
    pub relative_path: String,
    pub language: String,
    pub observations: Vec<StoredObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredLocalSymbolRef {
    pub relative_path: String,
    pub symbol_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoredEdgeTarget {
    LocalFile { relative_path: String },
    LocalSymbol(StoredLocalSymbolRef),
    External { node_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredResolvedEdge {
    pub id: String,
    pub relationship: String,
    pub source_path: String,
    pub source_observation_id: String,
    pub source_category: String,
    pub source_range: ByteRange,
    pub source_symbol_id: Option<String>,
    pub target: Option<StoredEdgeTarget>,
    pub resolution: String,
    pub rule_version: String,
    pub resolver_version: String,
    pub candidate_count: u64,
    pub reason: String,
    pub evidence_json: String,
    pub limitations_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredExternalNode {
    pub id: String,
    pub language: String,
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredResolutionDiagnostic {
    pub relative_path: String,
    pub observation_id: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredResolutionGraph {
    pub generation_id: GenerationId,
    pub resolver_version: String,
    pub edges: Vec<StoredResolvedEdge>,
    pub external_nodes: Vec<StoredExternalNode>,
    pub diagnostics: Vec<StoredResolutionDiagnostic>,
    pub unresolved_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredGraphDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredGraphEdge {
    pub id: String,
    pub relationship: String,
    pub source_symbol_id: Option<String>,
    pub source_observation_id: String,
    pub target_symbol_id: Option<String>,
    pub resolution: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredGraphFact {
    pub edge: StoredGraphEdge,
    pub source_path: String,
    pub source_category: String,
    pub source_spelling: String,
    pub source_start_byte: u64,
    pub target_path: Option<String>,
    pub target_name: Option<String>,
    pub candidate_count: u64,
    pub rule_version: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredSearchAfter {
    pub tier: u8,
    pub fts_score: f64,
    pub relative_path: String,
    pub name: String,
    pub start_byte: u64,
    pub symbol_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredSearchPlan {
    pub query: String,
    pub folded_query: String,
    pub escaped_folded_prefix: String,
    pub fts_expression: String,
    pub language: Option<String>,
    pub kind: Option<String>,
    pub path_prefix: Option<String>,
    pub after: Option<StoredSearchAfter>,
    pub fetch_limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSymbolRecord {
    pub id: String,
    pub relative_path: String,
    pub language: String,
    pub kind: String,
    pub name: String,
    pub container: Option<String>,
    pub signature: Option<String>,
    pub start_byte: u64,
    pub end_byte: u64,
    pub syntax_start_byte: u64,
    pub syntax_end_byte: u64,
    pub content_hash: String,
    pub parse_status: String,
    pub attributes_json: String,
    pub limitations_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredSearchSymbol {
    pub symbol: StoredSymbolRecord,
    pub tier: u8,
    pub fts_score: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredReferenceRecord {
    pub edge_id: String,
    pub source_observation_id: String,
    pub relative_path: String,
    pub spelling: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub resolution: String,
    pub candidate_count: u64,
    pub rule_version: String,
    pub limitations_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredReferenceAfter {
    pub relative_path: String,
    pub start_byte: u64,
    pub edge_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredRelationRecord {
    pub id: String,
    pub relationship: String,
    pub source_symbol_id: Option<String>,
    pub source_observation_id: String,
    pub source_path: String,
    pub source_start_byte: u64,
    pub target_symbol_id: Option<String>,
    pub target_path: Option<String>,
    pub resolution: String,
    pub candidate_count: u64,
    pub rule_version: String,
    pub reason: String,
    pub limitations_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredIndexedFile {
    pub relative_path: String,
    pub content_hash: String,
    pub language: String,
    pub parse_status: String,
    pub coverage_json: String,
    pub byte_length: u64,
    pub symbol_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredCoverage {
    pub status: String,
    pub warning_count: u64,
    pub unresolved_occurrences: u64,
    pub resolver_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexSummary {
    pub active_generation_id: Option<GenerationId>,
    pub active_files: u64,
    pub active_facts: u64,
    pub active_symbols: u64,
    pub latest_job: Option<JobRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFact {
    pub id: String,
    pub relative_path: String,
    pub kind: String,
    pub name: String,
    pub start_byte: u64,
    pub end_byte: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub name: String,
    pub relative_path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryKind {
    Decision,
    Convention,
    Pitfall,
    Task,
}

impl MemoryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Convention => "convention",
            Self::Pitfall => "pitfall",
            Self::Task => "task",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "decision" => Ok(Self::Decision),
            "convention" => Ok(Self::Convention),
            "pitfall" => Ok(Self::Pitfall),
            "task" => Ok(Self::Task),
            _ => Err(StorageError::InvalidInput(format!(
                "unknown memory kind '{value}'"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryEvidenceStatus {
    Verified,
    Unverified,
    Stale,
}

impl MemoryEvidenceStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Stale => "stale",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEvidence {
    pub relative_path: String,
    pub content_hash: String,
    pub symbol_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryDraft {
    pub memory_id: Option<String>,
    pub text: String,
    pub kind: MemoryKind,
    pub author: String,
    pub origin: String,
    pub scope: String,
    pub evidence: Vec<MemoryEvidence>,
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRecord {
    pub id: String,
    pub text: String,
    pub kind: MemoryKind,
    pub revision: u64,
    pub author: String,
    pub origin: String,
    pub scope: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub evidence: Vec<MemoryEvidence>,
    pub evidence_status: MemoryEvidenceStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySearch {
    pub query: String,
    pub scope: Option<String>,
    pub include_stale: bool,
    pub limit: usize,
}

pub struct Storage {
    role: StorageRole,
    repository_id: RepositoryId,
    paths: StoragePaths,
    capabilities: Option<SqliteCapabilities>,
    writer: Option<Writer>,
}

impl Storage {
    pub fn open(repository_id: RepositoryId, paths: StoragePaths) -> Result<Self, StorageError> {
        paths.prepare()?;
        reject_link_like(paths.lock_path())?;
        reject_link_like(paths.database_path())?;
        let mut lock_options = OpenOptions::new();
        lock_options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            lock_options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            lock_options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let lock = lock_options
            .open(paths.lock_path())
            .map_err(|source| io_error(paths.lock_path(), source))?;
        set_open_file_permissions(&lock, paths.lock_path())?;
        match fs4::FileExt::try_lock(&lock) {
            Ok(()) => {
                let writer = Writer::start(
                    repository_id.clone(),
                    paths.database_path().to_owned(),
                    lock,
                )?;
                let capabilities = Some(writer.capabilities.clone());
                Ok(Self {
                    role: StorageRole::Owner,
                    repository_id,
                    paths,
                    capabilities,
                    writer: Some(writer),
                })
            }
            Err(TryLockError::WouldBlock) => {
                let capabilities = if paths.database_path().is_file() {
                    Some(validate_follower_database(
                        paths.database_path(),
                        repository_id.as_str(),
                    )?)
                } else {
                    None
                };
                Ok(Self {
                    role: StorageRole::Follower,
                    repository_id,
                    paths,
                    capabilities,
                    writer: None,
                })
            }
            Err(TryLockError::Error(source)) => Err(io_error(paths.lock_path(), source)),
        }
    }

    #[must_use]
    pub const fn role(&self) -> StorageRole {
        self.role
    }

    #[must_use]
    pub fn paths(&self) -> &StoragePaths {
        &self.paths
    }

    #[must_use]
    pub fn capabilities(&self) -> Option<&SqliteCapabilities> {
        self.capabilities.as_ref()
    }

    pub fn create_job(&self, request: NewJob) -> Result<CreatedJob, StorageError> {
        validate_new_job(&request)?;
        let Some(writer) = &self.writer else {
            return Err(StorageError::WriterBusy {
                retry_after_ms: BUSY_TIMEOUT.as_millis() as u64,
            });
        };
        let (reply, response) = mpsc::sync_channel(1);
        match writer
            .sender
            .try_send(Command::CreateJob { request, reply })
        {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(StorageError::QueueFull),
            Err(TrySendError::Disconnected(_)) => return Err(StorageError::WriterDisconnected),
        }
        response
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => StorageError::WriterTimeout {
                    operation: "create_job",
                },
                mpsc::RecvTimeoutError::Disconnected => StorageError::WriterDisconnected,
            })?
    }

    pub fn update_job(
        &self,
        job_id: JobId,
        state: JobState,
        progress: JobProgress,
        error_summary: Option<String>,
    ) -> Result<(), StorageError> {
        validate_job_update(&progress, error_summary.as_deref())?;
        self.send_mutation(|reply| Command::UpdateJob {
            job_id,
            state,
            progress,
            error_summary,
            reply,
        })
    }

    pub fn request_cancel(&self, job_id: JobId) -> Result<CancelJobResult, StorageError> {
        let Some(writer) = &self.writer else {
            return Err(StorageError::WriterBusy {
                retry_after_ms: BUSY_TIMEOUT.as_millis() as u64,
            });
        };
        let (reply, response) = mpsc::sync_channel(1);
        match writer
            .sender
            .try_send(Command::RequestCancel { job_id, reply })
        {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(StorageError::QueueFull),
            Err(TrySendError::Disconnected(_)) => return Err(StorageError::WriterDisconnected),
        }
        response
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => StorageError::WriterTimeout {
                    operation: "request_cancel",
                },
                mpsc::RecvTimeoutError::Disconnected => StorageError::WriterDisconnected,
            })?
    }

    pub fn begin_generation(&self, plan: GenerationPlan) -> Result<(), StorageError> {
        validate_generation_plan(&plan)?;
        self.send_mutation(|reply| Command::BeginGeneration { plan, reply })
    }

    pub fn stage_files(
        &self,
        generation_id: GenerationId,
        files: Vec<StagedFile>,
    ) -> Result<(), StorageError> {
        validate_staged_files(&files)?;
        self.send_mutation(|reply| Command::StageFiles {
            generation_id,
            files,
            reply,
        })
    }

    pub fn reuse_files(
        &self,
        generation_id: GenerationId,
        relative_paths: Vec<String>,
    ) -> Result<(), StorageError> {
        validate_reuse_files(&relative_paths)?;
        self.send_mutation(|reply| Command::ReuseFiles {
            generation_id,
            relative_paths,
            reply,
        })
    }

    pub fn replace_resolution_graph(
        &self,
        graph: StoredResolutionGraph,
    ) -> Result<(), StorageError> {
        validate_resolution_graph(&graph)?;
        self.send_mutation(|reply| Command::ReplaceResolutionGraph { graph, reply })
    }

    pub fn complete_generation(
        &self,
        completion: GenerationCompletion,
    ) -> Result<(), StorageError> {
        validate_generation_completion(&completion)?;
        self.send_mutation(|reply| Command::CompleteGeneration { completion, reply })
    }

    pub fn activate_generation(&self, generation_id: GenerationId) -> Result<(), StorageError> {
        self.send_mutation(|reply| Command::Activate {
            generation_id,
            reply,
        })
    }

    pub fn abandon_generation(
        &self,
        generation_id: Option<GenerationId>,
        job_id: JobId,
        state: JobState,
        progress: JobProgress,
        error_summary: String,
    ) -> Result<(), StorageError> {
        if !matches!(state, JobState::Cancelled | JobState::Failed) {
            return Err(StorageError::InvalidInput(
                "abandoned jobs must be cancelled or failed".to_owned(),
            ));
        }
        validate_job_update(&progress, Some(&error_summary))?;
        self.send_mutation(|reply| Command::AbandonGeneration {
            generation_id,
            job_id,
            state,
            progress,
            error_summary,
            reply,
        })
    }

    pub fn gc_abandoned(&self) -> Result<(), StorageError> {
        self.send_mutation(|reply| Command::GcAbandoned { reply })
    }

    pub fn put_memory(&self, id: String, body: String) -> Result<(), StorageError> {
        validate_memory(&id, &body)?;
        self.send_mutation(|reply| Command::PutMemory { id, body, reply })
    }

    pub fn upsert_memory(&self, draft: MemoryDraft) -> Result<MemoryRecord, StorageError> {
        validate_memory_draft(&draft)?;
        let Some(writer) = &self.writer else {
            return Err(StorageError::WriterBusy {
                retry_after_ms: BUSY_TIMEOUT.as_millis() as u64,
            });
        };
        let (reply, response) = mpsc::sync_channel(1);
        match writer
            .sender
            .try_send(Command::UpsertMemory { draft, reply })
        {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(StorageError::QueueFull),
            Err(TrySendError::Disconnected(_)) => return Err(StorageError::WriterDisconnected),
        }
        response
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => StorageError::WriterTimeout {
                    operation: "upsert_memory",
                },
                mpsc::RecvTimeoutError::Disconnected => StorageError::WriterDisconnected,
            })?
    }

    pub fn forget_memory(
        &self,
        id: String,
        expected_revision: u64,
    ) -> Result<MemoryRecord, StorageError> {
        validate_memory_id(&id)?;
        if expected_revision == 0 {
            return Err(StorageError::InvalidInput(
                "expected_revision must be at least one".to_owned(),
            ));
        }
        let Some(writer) = &self.writer else {
            return Err(StorageError::WriterBusy {
                retry_after_ms: BUSY_TIMEOUT.as_millis() as u64,
            });
        };
        let (reply, response) = mpsc::sync_channel(1);
        match writer.sender.try_send(Command::ForgetMemory {
            id,
            expected_revision,
            reply,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(StorageError::QueueFull),
            Err(TrySendError::Disconnected(_)) => return Err(StorageError::WriterDisconnected),
        }
        response
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => StorageError::WriterTimeout {
                    operation: "forget_memory",
                },
                mpsc::RecvTimeoutError::Disconnected => StorageError::WriterDisconnected,
            })?
    }

    pub fn backup_to(&self, destination: PathBuf) -> Result<(), StorageError> {
        if destination.exists() {
            return Err(StorageError::BackupExists(destination));
        }
        let destination = resolve_backup_destination(&destination)?;
        if destination == self.paths.database_path() {
            return Err(StorageError::InvalidInput(
                "backup destination must differ from the live database".to_owned(),
            ));
        }
        self.send_mutation(|reply| Command::Backup { destination, reply })
    }

    pub fn read_snapshot(&self) -> Result<ReadSnapshot, StorageError> {
        ReadSnapshot::open(self.paths.database_path(), self.repository_id.as_str())
    }

    pub fn job_status(&self, job_id: &JobId) -> Result<JobRecord, StorageError> {
        if !self.paths.database_path().is_file() {
            return Err(StorageError::JobNotFound(job_id.as_str().to_owned()));
        }
        let connection = open_reader_connection(self.paths.database_path())?;
        validate_schema_and_root(&connection, self.repository_id.as_str())?;
        read_job(&connection, job_id.as_str())?
            .ok_or_else(|| StorageError::JobNotFound(job_id.as_str().to_owned()))
    }

    pub fn cancellation_requested(&self, job_id: &JobId) -> Result<bool, StorageError> {
        Ok(self.job_status(job_id)?.cancel_requested)
    }

    pub fn active_files(&self) -> Result<Vec<ActiveFile>, StorageError> {
        if !self.paths.database_path().is_file() {
            return Ok(Vec::new());
        }
        let connection = open_reader_connection(self.paths.database_path())?;
        validate_schema_and_root(&connection, self.repository_id.as_str())?;
        let mut statement = connection.prepare(
            "SELECT files.relative_path, fv.content_hash, fv.grammar_hash, fv.query_hash,
                    fv.extractor_hash, fv.config_hash
             FROM meta
             JOIN generations ON generations.id = meta.active_generation_id
             JOIN generation_files AS gf ON gf.generation_id = meta.active_generation_id
             JOIN files ON files.id = gf.file_id
             JOIN file_versions AS fv ON fv.id = gf.file_version_id
             WHERE meta.singleton = 1
             ORDER BY files.relative_path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ActiveFile {
                relative_path: row.get(0)?,
                content_hash: row.get(1)?,
                grammar_hash: row.get(2)?,
                query_hash: row.get(3)?,
                extractor_hash: row.get(4)?,
                config_hash: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn generation_resolution_files(
        &self,
        generation_id: &GenerationId,
    ) -> Result<Vec<StoredResolutionFile>, StorageError> {
        if !self.paths.database_path().is_file() {
            return Err(StorageError::IndexNotReady);
        }
        let connection = open_reader_connection(self.paths.database_path())?;
        validate_schema_and_root(&connection, self.repository_id.as_str())?;
        read_generation_resolution_files(&connection, generation_id)
    }

    pub fn index_summary(&self) -> Result<IndexSummary, StorageError> {
        if !self.paths.database_path().is_file() {
            return Ok(IndexSummary {
                active_generation_id: None,
                active_files: 0,
                active_facts: 0,
                active_symbols: 0,
                latest_job: None,
            });
        }
        let connection = open_reader_connection(self.paths.database_path())?;
        validate_schema_and_root(&connection, self.repository_id.as_str())?;
        read_index_summary(&connection)
    }

    pub fn read_memory(&self, id: &str) -> Result<Option<String>, StorageError> {
        validate_memory_id(id)?;
        if !self.paths.database_path().is_file() {
            return Ok(None);
        }
        let connection = open_reader_connection(self.paths.database_path())?;
        validate_schema_and_root(&connection, self.repository_id.as_str())?;
        connection
            .query_row("SELECT body FROM memories WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(StorageError::from)
    }

    pub fn memory(&self, id: &str) -> Result<Option<MemoryRecord>, StorageError> {
        validate_memory_id(id)?;
        if !self.paths.database_path().is_file() {
            return Ok(None);
        }
        let connection = open_reader_connection(self.paths.database_path())?;
        validate_schema_and_root(&connection, self.repository_id.as_str())?;
        read_memory_record(&connection, self.repository_id.as_str(), id)
    }

    pub fn search_memories(
        &self,
        request: &MemorySearch,
    ) -> Result<Vec<MemoryRecord>, StorageError> {
        validate_memory_search(request)?;
        if !self.paths.database_path().is_file() {
            return Ok(Vec::new());
        }
        let connection = open_reader_connection(self.paths.database_path())?;
        validate_schema_and_root(&connection, self.repository_id.as_str())?;
        search_memory_records(&connection, self.repository_id.as_str(), request)
    }

    fn send_mutation(
        &self,
        command: impl FnOnce(SyncSender<Result<(), StorageError>>) -> Command,
    ) -> Result<(), StorageError> {
        let Some(writer) = &self.writer else {
            return Err(StorageError::WriterBusy {
                retry_after_ms: BUSY_TIMEOUT.as_millis() as u64,
            });
        };
        let (reply, response) = mpsc::sync_channel(1);
        let command = command(reply);
        let operation = command.operation();
        match writer.sender.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(StorageError::QueueFull),
            Err(TrySendError::Disconnected(_)) => return Err(StorageError::WriterDisconnected),
        }
        response
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => StorageError::WriterTimeout { operation },
                mpsc::RecvTimeoutError::Disconnected => StorageError::WriterDisconnected,
            })?
    }
}

fn resolve_backup_destination(destination: &Path) -> Result<PathBuf, StorageError> {
    let name = destination.file_name().ok_or_else(|| {
        StorageError::InvalidInput("backup destination has no file name".to_owned())
    })?;
    let parent = destination.parent().ok_or_else(|| {
        StorageError::InvalidInput("backup destination has no parent directory".to_owned())
    })?;
    let canonical_parent = fs::canonicalize(parent).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            StorageError::InvalidInput(format!(
                "backup parent is not an existing directory: {}",
                parent.display()
            ))
        } else {
            io_error(parent, error)
        }
    })?;
    if !canonical_parent.is_dir() {
        return Err(StorageError::InvalidInput(format!(
            "backup parent is not an existing directory: {}",
            parent.display()
        )));
    }
    Ok(canonical_parent.join(name))
}

struct Writer {
    sender: SyncSender<Command>,
    join: Option<JoinHandle<()>>,
    capabilities: SqliteCapabilities,
}

impl Writer {
    fn start(
        repository_id: RepositoryId,
        database_path: PathBuf,
        lock: File,
    ) -> Result<Self, StorageError> {
        let (sender, receiver) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
        let (initialized, startup) = mpsc::sync_channel(1);
        let short_id: String = repository_id.as_str().chars().take(12).collect();
        let thread_name = format!("codeatlas-writer-{short_id}");
        let join = thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                writer_main(&repository_id, &database_path, receiver, initialized, lock);
            })
            .map_err(|source| io_error("writer thread", source))?;
        let capabilities = match startup.recv_timeout(COMMAND_TIMEOUT) {
            Ok(Ok(capabilities)) => capabilities,
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(StorageError::WriterTimeout {
                    operation: "writer_startup",
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = join.join();
                return Err(StorageError::WriterDisconnected);
            }
        };
        Ok(Self {
            sender,
            join: Some(join),
            capabilities,
        })
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

enum Command {
    CreateJob {
        request: NewJob,
        reply: SyncSender<Result<CreatedJob, StorageError>>,
    },
    UpdateJob {
        job_id: JobId,
        state: JobState,
        progress: JobProgress,
        error_summary: Option<String>,
        reply: SyncSender<Result<(), StorageError>>,
    },
    RequestCancel {
        job_id: JobId,
        reply: SyncSender<Result<CancelJobResult, StorageError>>,
    },
    BeginGeneration {
        plan: GenerationPlan,
        reply: SyncSender<Result<(), StorageError>>,
    },
    StageFiles {
        generation_id: GenerationId,
        files: Vec<StagedFile>,
        reply: SyncSender<Result<(), StorageError>>,
    },
    ReuseFiles {
        generation_id: GenerationId,
        relative_paths: Vec<String>,
        reply: SyncSender<Result<(), StorageError>>,
    },
    ReplaceResolutionGraph {
        graph: StoredResolutionGraph,
        reply: SyncSender<Result<(), StorageError>>,
    },
    CompleteGeneration {
        completion: GenerationCompletion,
        reply: SyncSender<Result<(), StorageError>>,
    },
    Activate {
        generation_id: GenerationId,
        reply: SyncSender<Result<(), StorageError>>,
    },
    PutMemory {
        id: String,
        body: String,
        reply: SyncSender<Result<(), StorageError>>,
    },
    UpsertMemory {
        draft: MemoryDraft,
        reply: SyncSender<Result<MemoryRecord, StorageError>>,
    },
    ForgetMemory {
        id: String,
        expected_revision: u64,
        reply: SyncSender<Result<MemoryRecord, StorageError>>,
    },
    Backup {
        destination: PathBuf,
        reply: SyncSender<Result<(), StorageError>>,
    },
    AbandonGeneration {
        generation_id: Option<GenerationId>,
        job_id: JobId,
        state: JobState,
        progress: JobProgress,
        error_summary: String,
        reply: SyncSender<Result<(), StorageError>>,
    },
    GcAbandoned {
        reply: SyncSender<Result<(), StorageError>>,
    },
    Shutdown,
}

impl Command {
    const fn operation(&self) -> &'static str {
        match self {
            Self::CreateJob { .. } => "create_job",
            Self::UpdateJob { .. } => "update_job",
            Self::RequestCancel { .. } => "request_cancel",
            Self::BeginGeneration { .. } => "begin_generation",
            Self::StageFiles { .. } => "stage_files",
            Self::ReuseFiles { .. } => "reuse_files",
            Self::ReplaceResolutionGraph { .. } => "replace_resolution_graph",
            Self::CompleteGeneration { .. } => "complete_generation",
            Self::Activate { .. } => "activate_generation",
            Self::PutMemory { .. } => "put_memory",
            Self::UpsertMemory { .. } => "upsert_memory",
            Self::ForgetMemory { .. } => "forget_memory",
            Self::Backup { .. } => "backup",
            Self::AbandonGeneration { .. } => "abandon_generation",
            Self::GcAbandoned { .. } => "gc_abandoned",
            Self::Shutdown => "shutdown",
        }
    }
}

fn writer_main(
    repository_id: &RepositoryId,
    database_path: &Path,
    receiver: Receiver<Command>,
    initialized: SyncSender<Result<SqliteCapabilities, StorageError>>,
    _lock: File,
) {
    let opened = open_writer_connection(database_path, repository_id.as_str());
    let (mut connection, capabilities) = match opened {
        Ok(value) => value,
        Err(error) => {
            let _ = initialized.send(Err(error));
            return;
        }
    };
    if initialized.send(Ok(capabilities)).is_err() {
        return;
    }
    for command in receiver {
        match command {
            Command::CreateJob { request, reply } => {
                let _ = reply.send(create_job(&mut connection, &request));
            }
            Command::UpdateJob {
                job_id,
                state,
                progress,
                error_summary,
                reply,
            } => {
                let _ = reply.send(update_job(
                    &mut connection,
                    &job_id,
                    state,
                    &progress,
                    error_summary.as_deref(),
                ));
            }
            Command::RequestCancel { job_id, reply } => {
                let _ = reply.send(request_cancel(&mut connection, &job_id));
            }
            Command::BeginGeneration { plan, reply } => {
                let _ = reply.send(begin_generation(&mut connection, &plan));
            }
            Command::StageFiles {
                generation_id,
                files,
                reply,
            } => {
                let _ = reply.send(stage_files(&mut connection, &generation_id, &files));
            }
            Command::ReuseFiles {
                generation_id,
                relative_paths,
                reply,
            } => {
                let _ = reply.send(reuse_files(
                    &mut connection,
                    &generation_id,
                    &relative_paths,
                ));
            }
            Command::ReplaceResolutionGraph { graph, reply } => {
                let _ = reply.send(replace_resolution_graph(&mut connection, &graph));
            }
            Command::CompleteGeneration { completion, reply } => {
                let _ = reply.send(complete_generation(&mut connection, &completion));
            }
            Command::Activate {
                generation_id,
                reply,
            } => {
                let _ = reply.send(activate_generation(&mut connection, &generation_id));
            }
            Command::PutMemory { id, body, reply } => {
                let _ = reply.send(put_memory(&mut connection, &id, &body));
            }
            Command::UpsertMemory { draft, reply } => {
                let _ = reply.send(upsert_memory(
                    &mut connection,
                    repository_id.as_str(),
                    &draft,
                ));
            }
            Command::ForgetMemory {
                id,
                expected_revision,
                reply,
            } => {
                let _ = reply.send(forget_memory(
                    &mut connection,
                    repository_id.as_str(),
                    &id,
                    expected_revision,
                ));
            }
            Command::Backup { destination, reply } => {
                let _ = reply.send(backup_database(&connection, &destination));
            }
            Command::AbandonGeneration {
                generation_id,
                job_id,
                state,
                progress,
                error_summary,
                reply,
            } => {
                let _ = reply.send(abandon_generation(
                    &mut connection,
                    generation_id.as_ref(),
                    &job_id,
                    state,
                    &progress,
                    &error_summary,
                ));
            }
            Command::GcAbandoned { reply } => {
                let _ = reply.send(gc_abandoned(&mut connection));
            }
            Command::Shutdown => break,
        }
    }
}

fn open_writer_connection(
    database_path: &Path,
    repository_id: &str,
) -> Result<(Connection, SqliteCapabilities), StorageError> {
    let mut connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    set_file_permissions(database_path)?;
    configure_connection(&connection, false)?;
    let version = probe_sqlite(&connection)?;
    schema::preflight(&connection, repository_id)?;
    let journal_mode: String =
        connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(StorageError::WalUnavailable {
            found: journal_mode,
        });
    }
    connection.pragma_update(None, "synchronous", "FULL")?;
    schema::migrate(&mut connection, repository_id)?;
    recover_interrupted_work(&connection)?;
    Ok((
        connection,
        SqliteCapabilities {
            version,
            fts5: true,
            journal_mode: "wal".to_owned(),
            schema_version: CURRENT_SCHEMA_VERSION,
        },
    ))
}

fn validate_follower_database(
    database_path: &Path,
    repository_id: &str,
) -> Result<SqliteCapabilities, StorageError> {
    let connection = open_reader_connection(database_path)?;
    let version = probe_sqlite(&connection)?;
    validate_schema_and_root(&connection, repository_id)?;
    let journal_mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(StorageError::WalUnavailable {
            found: journal_mode,
        });
    }
    Ok(SqliteCapabilities {
        version,
        fts5: true,
        journal_mode,
        schema_version: CURRENT_SCHEMA_VERSION,
    })
}

fn open_reader_connection(database_path: &Path) -> Result<Connection, StorageError> {
    let connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    configure_connection(&connection, true)?;
    Ok(connection)
}

fn configure_connection(connection: &Connection, query_only: bool) -> Result<(), StorageError> {
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    if query_only {
        connection.pragma_update(None, "query_only", "ON")?;
    }
    Ok(())
}

fn probe_sqlite(connection: &Connection) -> Result<String, StorageError> {
    let version: String = connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    if parse_version(&version)? < MIN_SQLITE_VERSION {
        return Err(StorageError::SqliteTooOld { found: version });
    }
    let fts5: i64 = connection.query_row(
        "SELECT sqlite_compileoption_used('ENABLE_FTS5')",
        [],
        |row| row.get(0),
    )?;
    if fts5 != 1 {
        return Err(StorageError::Fts5Unavailable);
    }
    Ok(version)
}

fn parse_version(version: &str) -> Result<(u32, u32, u32), StorageError> {
    let mut components = version.split('.');
    let parse = |component: Option<&str>| {
        component
            .ok_or_else(|| {
                StorageError::InvalidInput(format!("invalid SQLite version '{version}'"))
            })?
            .parse::<u32>()
            .map_err(|_| StorageError::InvalidInput(format!("invalid SQLite version '{version}'")))
    };
    Ok((
        parse(components.next())?,
        parse(components.next())?,
        parse(components.next())?,
    ))
}

fn validate_schema_and_root(
    connection: &Connection,
    repository_id: &str,
) -> Result<(), StorageError> {
    let version = schema::preflight(connection, repository_id)?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::FutureSchema {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    if version < CURRENT_SCHEMA_VERSION {
        return Err(StorageError::MigrationRequired {
            found: version,
            required: CURRENT_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn recover_interrupted_work(connection: &Connection) -> Result<(), StorageError> {
    let now = now_unix()?;
    let transaction = connection.unchecked_transaction()?;
    transaction.execute(
        "UPDATE generations SET status = 'abandoned' WHERE status = 'building'",
        [],
    )?;
    transaction.execute(
        "UPDATE jobs SET status = 'interrupted', updated_at = ?1,
         error_summary = 'writer stopped before completion'
         WHERE status IN ('queued', 'scanning', 'parsing', 'resolving', 'committing')",
        [now],
    )?;
    transaction.commit()?;
    Ok(())
}

fn validate_new_job(request: &NewJob) -> Result<(), StorageError> {
    if !matches!(request.mode.as_str(), "incremental" | "full") {
        return Err(StorageError::InvalidInput(
            "job mode must be incremental or full".to_owned(),
        ));
    }
    if request.owner_instance_id.is_empty() || request.owner_instance_id.len() > 128 {
        return Err(StorageError::InvalidInput(
            "owner instance ID must be 1..=128 bytes".to_owned(),
        ));
    }
    if request.request_key.as_ref().is_some_and(|key| {
        key.is_empty() || key.len() > MAX_REQUEST_KEY_BYTES || key.contains('\0')
    }) {
        return Err(StorageError::InvalidInput(
            "request key is empty or exceeds policy".to_owned(),
        ));
    }
    Ok(())
}

fn validate_job_update(
    progress: &JobProgress,
    error_summary: Option<&str>,
) -> Result<(), StorageError> {
    for value in [
        progress.files_discovered,
        progress.files_reused,
        progress.files_parsed,
        progress.files_failed,
        progress.files_persisted,
        progress.files_deleted,
        progress.warning_count,
    ] {
        i64::try_from(value).map_err(|_| {
            StorageError::InvalidInput("job progress exceeds SQLite INTEGER".to_owned())
        })?;
    }
    if error_summary.is_some_and(|summary| summary.len() > MAX_JOB_ERROR_BYTES) {
        return Err(StorageError::InvalidInput(
            "job error summary exceeds policy".to_owned(),
        ));
    }
    Ok(())
}

fn validate_generation_plan(plan: &GenerationPlan) -> Result<(), StorageError> {
    for (name, value) in [
        ("config hash", plan.config_hash.as_str()),
        ("extractor-set hash", plan.extractor_set_hash.as_str()),
    ] {
        if value.is_empty() || value.len() > 128 {
            return Err(StorageError::InvalidInput(format!(
                "{name} must be 1..=128 bytes"
            )));
        }
    }
    if plan.deleted_paths.len() > 100_000 {
        return Err(StorageError::InvalidInput(
            "generation deletion set exceeds 100000 paths".to_owned(),
        ));
    }
    let mut unique = HashSet::with_capacity(plan.deleted_paths.len());
    for path in &plan.deleted_paths {
        validate_relative_path(path)?;
        if !unique.insert(path) {
            return Err(StorageError::InvalidInput(
                "generation deletion set contains duplicate paths".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_generation_completion(completion: &GenerationCompletion) -> Result<(), StorageError> {
    if !matches!(
        completion.coverage_status.as_str(),
        "syntax_only" | "syntax_and_name_resolution" | "ready_with_warnings" | "failed"
    ) {
        return Err(StorageError::InvalidInput(
            "invalid generation coverage status".to_owned(),
        ));
    }
    i64::try_from(completion.warning_count).map_err(|_| {
        StorageError::InvalidInput("warning count exceeds SQLite INTEGER".to_owned())
    })?;
    if completion
        .failure_summary
        .as_ref()
        .is_some_and(|summary| summary.len() > MAX_JOB_ERROR_BYTES)
    {
        return Err(StorageError::InvalidInput(
            "generation failure summary exceeds policy".to_owned(),
        ));
    }
    Ok(())
}

fn validate_staged_files(files: &[StagedFile]) -> Result<(), StorageError> {
    if files.len() > MAX_STAGE_FILES {
        return Err(StorageError::InvalidInput(format!(
            "generation contains {} files; maximum per batch is {MAX_STAGE_FILES}",
            files.len()
        )));
    }
    let mut paths = HashSet::with_capacity(files.len());
    for file in files {
        validate_relative_path(&file.relative_path)?;
        if !paths.insert(file.relative_path.as_str()) {
            return Err(StorageError::InvalidInput(
                "staged file batch contains duplicate paths".to_owned(),
            ));
        }
        if file.content_hash.len() != 64
            || !file
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(StorageError::InvalidInput(
                "content hash must be 64 hexadecimal characters".to_owned(),
            ));
        }
        if file.extractor_hash.is_empty()
            || file.extractor_hash.len() > 128
            || file.grammar_hash.is_empty()
            || file.grammar_hash.len() > 128
            || file.query_hash.is_empty()
            || file.query_hash.len() > 128
            || file.language.is_empty()
            || file.language.len() > 32
        {
            return Err(StorageError::InvalidInput(
                "file language/fingerprint fields exceed policy".to_owned(),
            ));
        }
        if !matches!(
            file.parse_status.as_str(),
            "complete" | "partial" | "failed"
        ) {
            return Err(StorageError::InvalidInput(
                "invalid file parse status".to_owned(),
            ));
        }
        if file.coverage_json.len() > 16 * 1024 {
            return Err(StorageError::InvalidInput(
                "file coverage exceeds policy".to_owned(),
            ));
        }
        if file.facts.len() > MAX_FACTS_PER_FILE
            || file.observations.len() > MAX_FACTS_PER_FILE
            || file.search_documents.len() > MAX_SEARCH_DOCUMENTS_PER_FILE
        {
            return Err(StorageError::InvalidInput(
                "file fact/search-document limit exceeded".to_owned(),
            ));
        }
        for observation in &file.observations {
            if observation.id.is_empty()
                || observation.id.len() > 256
                || observation.kind.is_empty()
                || observation.kind.len() > 64
                || observation.spelling.len() > 4_096
                || observation.attributes_json.len() > 16 * 1024
                || observation.limitations_json.len() > 16 * 1024
            {
                return Err(StorageError::InvalidInput(format!(
                    "invalid observation fields in '{}': id={} kind={} spelling={} attributes={} limitations={}",
                    file.relative_path,
                    observation.id.len(),
                    observation.kind.len(),
                    observation.spelling.len(),
                    observation.attributes_json.len(),
                    observation.limitations_json.len()
                )));
            }
            if observation.byte_range.end() > file.byte_length
                || observation.syntax_range.end() > file.byte_length
            {
                return Err(StorageError::InvalidInput(
                    "observation range exceeds its immutable file version".to_owned(),
                ));
            }
        }
        if file.diagnostics.len() > 1_000 {
            return Err(StorageError::InvalidInput(
                "file diagnostic limit exceeded".to_owned(),
            ));
        }
        for diagnostic in &file.diagnostics {
            if diagnostic.code.is_empty()
                || diagnostic.code.len() > 128
                || diagnostic.message.len() > MAX_JOB_ERROR_BYTES
                || !matches!(
                    diagnostic.severity.as_str(),
                    "information" | "warning" | "error"
                )
                || diagnostic
                    .byte_range
                    .is_some_and(|range| range.end() > file.byte_length)
            {
                return Err(StorageError::InvalidInput(
                    "invalid diagnostic fields".to_owned(),
                ));
            }
        }
        for fact in &file.facts {
            if fact.id.is_empty()
                || fact.id.len() > 256
                || fact.kind.is_empty()
                || fact.kind.len() > 64
                || fact.name.len() > 512
            {
                return Err(StorageError::InvalidInput("invalid fact fields".to_owned()));
            }
            if fact.byte_range.end() > file.byte_length {
                return Err(StorageError::InvalidInput(
                    "fact byte range exceeds its immutable file version".to_owned(),
                ));
            }
        }
        for document in &file.search_documents {
            if document.name.len() > 512 || document.content.len() > 16 * 1024 {
                return Err(StorageError::InvalidInput(
                    "search document exceeds its field limit".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_resolution_graph(graph: &StoredResolutionGraph) -> Result<(), StorageError> {
    if graph.resolver_version.is_empty() || graph.resolver_version.len() > 128 {
        return Err(StorageError::InvalidInput(
            "resolver version must be 1..=128 bytes".to_owned(),
        ));
    }
    if graph.edges.len() > MAX_RESOLUTION_EDGES
        || graph.external_nodes.len() > MAX_EXTERNAL_NODES
        || graph.diagnostics.len() > MAX_RESOLUTION_DIAGNOSTICS
    {
        return Err(StorageError::InvalidInput(
            "resolution graph exceeds persistence limits".to_owned(),
        ));
    }
    let mut edge_ids = HashSet::with_capacity(graph.edges.len());
    for edge in &graph.edges {
        validate_relative_path(&edge.source_path)?;
        if edge.id.is_empty()
            || edge.id.len() > 256
            || !edge_ids.insert(edge.id.as_str())
            || edge.source_observation_id.is_empty()
            || edge.source_observation_id.len() > 256
            || edge
                .source_symbol_id
                .as_ref()
                .is_some_and(|id| id.len() > 256)
            || !matches!(
                edge.relationship.as_str(),
                "contains" | "imports" | "references" | "calls"
            )
            || !matches!(
                edge.source_category.as_str(),
                "symbol" | "scope" | "import" | "reference" | "call_site"
            )
            || !matches!(
                edge.resolution.as_str(),
                "syntax_observation" | "lexically_resolved" | "candidate" | "unresolved"
            )
            || edge.rule_version.is_empty()
            || edge.rule_version.len() > 128
            || edge.resolver_version != graph.resolver_version
            || edge.reason.len() > MAX_JOB_ERROR_BYTES
            || edge.evidence_json.len() > 16 * 1024
            || edge.limitations_json.len() > 16 * 1024
        {
            return Err(StorageError::InvalidInput(
                "invalid resolved edge fields".to_owned(),
            ));
        }
        match &edge.target {
            Some(StoredEdgeTarget::LocalFile { relative_path }) => {
                validate_relative_path(relative_path)?;
            }
            Some(StoredEdgeTarget::LocalSymbol(target)) => {
                validate_relative_path(&target.relative_path)?;
                if target.symbol_id.is_empty() || target.symbol_id.len() > 256 {
                    return Err(StorageError::InvalidInput(
                        "invalid local edge target".to_owned(),
                    ));
                }
            }
            Some(StoredEdgeTarget::External { node_id }) => {
                if node_id.is_empty() || node_id.len() > 256 {
                    return Err(StorageError::InvalidInput(
                        "invalid external edge target".to_owned(),
                    ));
                }
            }
            None if edge.resolution != "unresolved" => {
                return Err(StorageError::InvalidInput(
                    "only unresolved edges may omit a target".to_owned(),
                ));
            }
            None => {}
        }
        if edge.resolution == "unresolved" && edge.target.is_some() {
            return Err(StorageError::InvalidInput(
                "unresolved edges cannot claim a target".to_owned(),
            ));
        }
        i64::try_from(edge.candidate_count).map_err(|_| {
            StorageError::InvalidInput("candidate count exceeds SQLite INTEGER".to_owned())
        })?;
    }
    let mut external_ids = HashSet::with_capacity(graph.external_nodes.len());
    for node in &graph.external_nodes {
        if node.id.is_empty()
            || node.id.len() > 256
            || !external_ids.insert(node.id.as_str())
            || node.language.is_empty()
            || node.language.len() > 32
            || node.kind.is_empty()
            || node.kind.len() > 64
            || node.label.len() > 4_096
        {
            return Err(StorageError::InvalidInput(
                "invalid external node fields".to_owned(),
            ));
        }
    }
    for diagnostic in &graph.diagnostics {
        validate_relative_path(&diagnostic.relative_path)?;
        if diagnostic.code.is_empty()
            || diagnostic.code.len() > 128
            || diagnostic.message.len() > MAX_JOB_ERROR_BYTES
            || diagnostic
                .observation_id
                .as_ref()
                .is_some_and(|id| id.len() > 256)
        {
            return Err(StorageError::InvalidInput(
                "invalid resolution diagnostic".to_owned(),
            ));
        }
    }
    i64::try_from(graph.unresolved_count).map_err(|_| {
        StorageError::InvalidInput("unresolved count exceeds SQLite INTEGER".to_owned())
    })?;
    Ok(())
}

fn validate_reuse_files(relative_paths: &[String]) -> Result<(), StorageError> {
    if relative_paths.len() > MAX_REUSE_FILES_PER_BATCH {
        return Err(StorageError::InvalidInput(format!(
            "reuse batch contains {} files; maximum is {MAX_REUSE_FILES_PER_BATCH}",
            relative_paths.len()
        )));
    }
    let mut unique = HashSet::with_capacity(relative_paths.len());
    for path in relative_paths {
        validate_relative_path(path)?;
        if !unique.insert(path) {
            return Err(StorageError::InvalidInput(
                "reuse batch contains duplicate paths".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), StorageError> {
    if path.is_empty()
        || path.len() > 4_096
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with("//")
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err(StorageError::InvalidInput(
            "invalid relative path".to_owned(),
        ));
    }
    if !Path::new(path)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(StorageError::InvalidInput(
            "invalid relative path".to_owned(),
        ));
    }
    Ok(())
}

fn validate_memory(id: &str, body: &str) -> Result<(), StorageError> {
    validate_memory_id(id)?;
    if body.is_empty() || body.len() > MAX_MEMORY_BYTES {
        return Err(StorageError::InvalidInput(
            "memory ID/body is empty or exceeds policy".to_owned(),
        ));
    }
    Ok(())
}

fn validate_memory_draft(draft: &MemoryDraft) -> Result<(), StorageError> {
    if let Some(id) = draft.memory_id.as_deref() {
        validate_memory_id(id)?;
    }
    if draft.text.is_empty() || draft.text.len() > MAX_MEMORY_BYTES || draft.text.contains('\0') {
        return Err(StorageError::InvalidInput(format!(
            "memory text must contain 1..={MAX_MEMORY_BYTES} UTF-8 bytes and no NUL"
        )));
    }
    for (name, value) in [
        ("author", draft.author.as_str()),
        ("origin", draft.origin.as_str()),
        ("scope", draft.scope.as_str()),
    ] {
        if value.is_empty()
            || value.len() > MAX_MEMORY_METADATA_BYTES
            || value.contains('\0')
            || value.chars().any(char::is_control)
        {
            return Err(StorageError::InvalidInput(format!(
                "memory {name} is empty or exceeds policy"
            )));
        }
    }
    if draft.evidence.len() > MAX_MEMORY_EVIDENCE {
        return Err(StorageError::InvalidInput(format!(
            "memory evidence exceeds the {MAX_MEMORY_EVIDENCE}-item limit"
        )));
    }
    let evidence_bytes = draft.evidence.iter().try_fold(0_usize, |total, item| {
        total
            .checked_add(item.relative_path.len())?
            .checked_add(item.content_hash.len())?
            .checked_add(item.symbol_id.as_deref().map_or(0, str::len))
    });
    if evidence_bytes.is_none_or(|bytes| bytes > MAX_MEMORY_EVIDENCE_BYTES) {
        return Err(StorageError::InvalidInput(format!(
            "memory evidence exceeds the {MAX_MEMORY_EVIDENCE_BYTES}-byte aggregate limit"
        )));
    }
    let mut seen = HashSet::new();
    for evidence in &draft.evidence {
        validate_relative_path(&evidence.relative_path)?;
        validate_hex_identifier(&evidence.content_hash, 64, "content hash")?;
        if let Some(symbol_id) = evidence.symbol_id.as_deref()
            && (symbol_id.is_empty()
                || symbol_id.len() > 128
                || !symbol_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
        {
            return Err(StorageError::InvalidInput(
                "memory evidence symbol ID is invalid or exceeds policy".to_owned(),
            ));
        }
        if !seen.insert((
            evidence.relative_path.as_str(),
            evidence.content_hash.as_str(),
            evidence.symbol_id.as_deref(),
        )) {
            return Err(StorageError::InvalidInput(
                "duplicate memory evidence is not allowed".to_owned(),
            ));
        }
    }
    if draft.expected_revision == Some(0) {
        return Err(StorageError::InvalidInput(
            "expected_revision must be at least one".to_owned(),
        ));
    }
    Ok(())
}

fn validate_memory_search(request: &MemorySearch) -> Result<(), StorageError> {
    if request.query.is_empty()
        || request.query.len() > MAX_SEARCH_QUERY_BYTES
        || request.query.contains('\0')
    {
        return Err(StorageError::InvalidInput(
            "memory search query is empty or exceeds policy".to_owned(),
        ));
    }
    if !(1..=100).contains(&request.limit) {
        return Err(StorageError::InvalidInput(
            "memory search limit must be in 1..=100".to_owned(),
        ));
    }
    if let Some(scope) = request.scope.as_deref()
        && (scope.is_empty()
            || scope.len() > MAX_MEMORY_METADATA_BYTES
            || scope.contains('\0')
            || scope.chars().any(char::is_control))
    {
        return Err(StorageError::InvalidInput(
            "memory search scope is empty or exceeds policy".to_owned(),
        ));
    }
    Ok(())
}

fn validate_hex_identifier(
    value: &str,
    exact_bytes: usize,
    label: &str,
) -> Result<(), StorageError> {
    if value.len() != exact_bytes || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StorageError::InvalidInput(format!(
            "memory evidence {label} must be {exact_bytes} hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_memory_id(id: &str) -> Result<(), StorageError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(StorageError::InvalidInput(
            "memory ID is invalid or exceeds policy".to_owned(),
        ));
    }
    Ok(())
}

fn create_job(connection: &mut Connection, request: &NewJob) -> Result<CreatedJob, StorageError> {
    let now = now_unix()?;
    let transaction = connection.transaction()?;
    if let Some(request_key) = request.request_key.as_deref()
        && let Some(existing) = read_job_by_request_key(&transaction, request_key)?
    {
        transaction.commit()?;
        return Ok(CreatedJob {
            job: existing,
            deduplicated: true,
        });
    }
    let counter = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StorageError::InvalidInput("system clock is before Unix epoch".to_owned()))?
        .as_nanos();
    let job_id = JobId::new(format!(
        "job-{nanos:032x}-{:08x}-{counter:016x}",
        std::process::id()
    ))
    .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
    transaction.execute(
        "INSERT INTO jobs(
            id, operation, mode, request_key, status, owner_instance_id,
            created_at, updated_at
         ) VALUES (?1, 'index', ?2, ?3, 'queued', ?4, ?5, ?5)",
        params![
            job_id.as_str(),
            request.mode,
            request.request_key,
            request.owner_instance_id,
            now
        ],
    )?;
    let record = read_job(&transaction, job_id.as_str())?.ok_or_else(|| {
        StorageError::InvalidInput("created job could not be read back".to_owned())
    })?;
    transaction.commit()?;
    Ok(CreatedJob {
        job: record,
        deduplicated: false,
    })
}

fn update_job(
    connection: &mut Connection,
    job_id: &JobId,
    state: JobState,
    progress: &JobProgress,
    error_summary: Option<&str>,
) -> Result<(), StorageError> {
    let now = now_unix()?;
    let transaction = connection.transaction()?;
    let current = read_job(&transaction, job_id.as_str())?
        .ok_or_else(|| StorageError::JobNotFound(job_id.as_str().to_owned()))?;
    validate_job_transition(current.state, state).map_err(|()| {
        StorageError::InvalidJobTransition {
            job: job_id.as_str().to_owned(),
            from: current.state.as_str().to_owned(),
            to: state.as_str().to_owned(),
        }
    })?;
    transaction.execute(
        "UPDATE jobs SET status = ?2,
            files_discovered = ?3, files_reused = ?4, files_parsed = ?5,
            files_failed = ?6, files_persisted = ?7, files_deleted = ?8,
            warning_count = ?9, error_summary = ?10, updated_at = ?11
         WHERE id = ?1",
        params![
            job_id.as_str(),
            state.as_str(),
            as_i64(progress.files_discovered, "files_discovered")?,
            as_i64(progress.files_reused, "files_reused")?,
            as_i64(progress.files_parsed, "files_parsed")?,
            as_i64(progress.files_failed, "files_failed")?,
            as_i64(progress.files_persisted, "files_persisted")?,
            as_i64(progress.files_deleted, "files_deleted")?,
            as_i64(progress.warning_count, "warning_count")?,
            error_summary,
            now,
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn validate_job_transition(from: JobState, to: JobState) -> Result<(), ()> {
    if from == to {
        return Ok(());
    }
    let valid = match from {
        JobState::Queued => matches!(
            to,
            JobState::Scanning | JobState::Cancelled | JobState::Failed
        ),
        JobState::Scanning => matches!(
            to,
            JobState::Parsing | JobState::Cancelled | JobState::Failed
        ),
        JobState::Parsing => matches!(
            to,
            JobState::Resolving | JobState::Cancelled | JobState::Failed
        ),
        JobState::Resolving => matches!(
            to,
            JobState::Committing | JobState::Cancelled | JobState::Failed
        ),
        JobState::Committing => matches!(to, JobState::Completed | JobState::Failed),
        JobState::Completed | JobState::Cancelled | JobState::Failed | JobState::Interrupted => {
            false
        }
    };
    valid.then_some(()).ok_or(())
}

fn request_cancel(
    connection: &mut Connection,
    job_id: &JobId,
) -> Result<CancelJobResult, StorageError> {
    let transaction = connection.transaction()?;
    let job = read_job(&transaction, job_id.as_str())?
        .ok_or_else(|| StorageError::JobNotFound(job_id.as_str().to_owned()))?;
    if job.state.is_terminal() {
        transaction.commit()?;
        return Ok(CancelJobResult::AlreadyCompleted);
    }
    if job.cancel_requested {
        transaction.commit()?;
        return Ok(CancelJobResult::AlreadyRequested);
    }
    transaction.execute(
        "UPDATE jobs SET cancel_requested = 1, updated_at = ?2 WHERE id = ?1",
        params![job_id.as_str(), now_unix()?],
    )?;
    transaction.commit()?;
    Ok(CancelJobResult::Requested)
}

fn begin_generation(
    connection: &mut Connection,
    plan: &GenerationPlan,
) -> Result<(), StorageError> {
    let now = now_unix()?;
    let transaction = connection.transaction()?;
    let job = read_job(&transaction, plan.job_id.as_str())?
        .ok_or_else(|| StorageError::JobNotFound(plan.job_id.as_str().to_owned()))?;
    if job.state != JobState::Parsing {
        return Err(StorageError::InvalidJobTransition {
            job: plan.job_id.as_str().to_owned(),
            from: job.state.as_str().to_owned(),
            to: "begin_generation".to_owned(),
        });
    }
    let parent: Option<String> = transaction.query_row(
        "SELECT active_generation_id FROM meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO generations(
            id, parent_id, status, scan_complete, created_at,
            coverage_status, config_hash, extractor_set_hash
         ) VALUES (?1, ?2, 'building', 0, ?3, 'syntax_only', ?4, ?5)",
        params![
            plan.generation_id.as_str(),
            parent.as_deref(),
            now,
            plan.config_hash,
            plan.extractor_set_hash,
        ],
    )?;
    if plan.reuse_parent
        && let Some(parent) = parent.as_deref()
    {
        transaction.execute(
            "INSERT INTO generation_files(generation_id, file_id, file_version_id)
             SELECT ?1, file_id, file_version_id
             FROM generation_files WHERE generation_id = ?2",
            params![plan.generation_id.as_str(), parent],
        )?;
        for path in &plan.deleted_paths {
            let deleted = transaction.execute(
                "DELETE FROM generation_files
                 WHERE generation_id = ?1
                   AND file_id = (SELECT id FROM files WHERE relative_path = ?2)",
                params![plan.generation_id.as_str(), path],
            )?;
            if deleted != 1 {
                return Err(StorageError::InvalidGeneration {
                    generation: plan.generation_id.as_str().to_owned(),
                    reason: "deleted file is absent from the parent generation",
                });
            }
        }
    }
    transaction.execute(
        "UPDATE jobs SET generation_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![plan.job_id.as_str(), plan.generation_id.as_str(), now],
    )?;
    transaction.commit()?;
    Ok(())
}

fn stage_files(
    connection: &mut Connection,
    generation_id: &GenerationId,
    files: &[StagedFile],
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    ensure_building_generation(&transaction, generation_id)?;
    for file in files {
        stage_file(&transaction, generation_id.as_str(), file)?;
    }
    transaction.commit()?;
    Ok(())
}

fn reuse_files(
    connection: &mut Connection,
    generation_id: &GenerationId,
    relative_paths: &[String],
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    ensure_building_generation(&transaction, generation_id)?;
    let parent: Option<String> = transaction.query_row(
        "SELECT parent_id FROM generations WHERE id = ?1",
        [generation_id.as_str()],
        |row| row.get(0),
    )?;
    let Some(parent) = parent else {
        if relative_paths.is_empty() {
            transaction.commit()?;
            return Ok(());
        }
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "generation has no active parent for reuse",
        });
    };
    {
        let mut statement = transaction.prepare(
            "INSERT INTO generation_files(generation_id, file_id, file_version_id)
             SELECT ?1, gf.file_id, gf.file_version_id
             FROM generation_files AS gf
             JOIN files ON files.id = gf.file_id
             WHERE gf.generation_id = ?2 AND files.relative_path = ?3",
        )?;
        for path in relative_paths {
            let inserted = statement.execute(params![generation_id.as_str(), parent, path])?;
            if inserted != 1 {
                return Err(StorageError::InvalidGeneration {
                    generation: generation_id.as_str().to_owned(),
                    reason: "requested reusable file is absent from the parent generation",
                });
            }
        }
    }
    transaction.commit()?;
    Ok(())
}

fn replace_resolution_graph(
    connection: &mut Connection,
    graph: &StoredResolutionGraph,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    ensure_building_generation(&transaction, &graph.generation_id)?;
    let generation = graph.generation_id.as_str();
    // Empty graphs are common for declaration-only corpora. Avoid materializing the
    // complete membership only when neither edges nor file-scoped diagnostics need it.
    let file_versions = if graph.edges.is_empty() && graph.diagnostics.is_empty() {
        Default::default()
    } else {
        generation_file_versions(&transaction, generation)?
    };
    transaction.execute(
        "DELETE FROM resolved_edges WHERE generation_id = ?1",
        [generation],
    )?;
    transaction.execute(
        "DELETE FROM resolution_diagnostics WHERE generation_id = ?1",
        [generation],
    )?;
    transaction.execute(
        "DELETE FROM external_nodes WHERE generation_id = ?1",
        [generation],
    )?;
    for node in &graph.external_nodes {
        transaction.execute(
            "INSERT INTO external_nodes(generation_id, node_id, language, kind, label)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![generation, node.id, node.language, node.kind, node.label],
        )?;
    }
    for edge in &graph.edges {
        let source_file_version_id =
            file_versions
                .get(&edge.source_path)
                .copied()
                .ok_or_else(|| {
                    StorageError::InvalidInput(format!(
                        "edge source '{}' is absent from generation membership",
                        edge.source_path
                    ))
                })?;
        if !source_observation_exists(
            &transaction,
            source_file_version_id,
            &edge.source_category,
            &edge.source_observation_id,
        )? {
            return Err(StorageError::InvalidInput(format!(
                "edge source observation '{}' is absent from its immutable file version",
                edge.source_observation_id
            )));
        }
        let (target_file_version_id, target_symbol_id, external_node_id) = match &edge.target {
            Some(StoredEdgeTarget::LocalFile { relative_path }) => (
                Some(*file_versions.get(relative_path).ok_or_else(|| {
                    StorageError::InvalidInput(format!(
                        "local edge target '{relative_path}' is absent from generation membership"
                    ))
                })?),
                None,
                None,
            ),
            Some(StoredEdgeTarget::LocalSymbol(target)) => (
                Some(*file_versions.get(&target.relative_path).ok_or_else(|| {
                    StorageError::InvalidInput(format!(
                        "local edge target '{}' is absent from generation membership",
                        target.relative_path
                    ))
                })?),
                Some(target.symbol_id.as_str()),
                None,
            ),
            Some(StoredEdgeTarget::External { node_id }) => (None, None, Some(node_id.as_str())),
            None => (None, None, None),
        };
        transaction.execute(
            "INSERT INTO resolved_edges(
                generation_id, edge_id, relationship, source_file_version_id,
                source_observation_id, source_category, source_start_byte, source_end_byte,
                source_symbol_id, target_file_version_id, target_symbol_id, external_node_id,
                resolution, rule_version, resolver_version, candidate_count, reason,
                evidence_json, limitations_json
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19
             )",
            params![
                generation,
                edge.id,
                edge.relationship,
                source_file_version_id,
                edge.source_observation_id,
                edge.source_category,
                range_start(edge.source_range, "edge source start")?,
                range_end(edge.source_range, "edge source end")?,
                edge.source_symbol_id,
                target_file_version_id,
                target_symbol_id,
                external_node_id,
                edge.resolution,
                edge.rule_version,
                edge.resolver_version,
                as_i64(edge.candidate_count, "candidate_count")?,
                edge.reason,
                edge.evidence_json,
                edge.limitations_json,
            ],
        )?;
    }
    for (ordinal, diagnostic) in graph.diagnostics.iter().enumerate() {
        let file_version_id = file_versions
            .get(&diagnostic.relative_path)
            .copied()
            .ok_or_else(|| {
                StorageError::InvalidInput(format!(
                    "resolution diagnostic path '{}' is absent from generation membership",
                    diagnostic.relative_path
                ))
            })?;
        transaction.execute(
            "INSERT INTO resolution_diagnostics(
                generation_id, ordinal, file_version_id, observation_id, code, message
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                generation,
                i64::try_from(ordinal).map_err(|_| StorageError::InvalidInput(
                    "resolution diagnostic ordinal exceeds SQLite INTEGER".to_owned()
                ))?,
                file_version_id,
                diagnostic.observation_id,
                diagnostic.code,
                diagnostic.message,
            ],
        )?;
    }
    transaction.execute(
        "UPDATE generations SET resolver_version = ?2, resolved_edge_count = ?3,
            unresolved_occurrence_count = ?4 WHERE id = ?1",
        params![
            generation,
            graph.resolver_version,
            i64::try_from(graph.edges.len()).map_err(|_| StorageError::InvalidInput(
                "resolved edge count exceeds SQLite INTEGER".to_owned()
            ))?,
            as_i64(graph.unresolved_count, "unresolved_count")?,
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn generation_file_versions(
    connection: &Connection,
    generation_id: &str,
) -> Result<BTreeMap<String, i64>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT files.relative_path, gf.file_version_id
         FROM generation_files AS gf
         JOIN files ON files.id = gf.file_id
         WHERE gf.generation_id = ?1
         ORDER BY files.relative_path",
    )?;
    let rows = statement.query_map([generation_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(StorageError::from)
}

fn source_observation_exists(
    connection: &Connection,
    file_version_id: i64,
    category: &str,
    observation_id: &str,
) -> Result<bool, StorageError> {
    let table = match category {
        "symbol" => "symbols",
        "scope" => "scopes",
        "import" => "imports",
        "reference" => "\"references\"",
        "call_site" => "call_sites",
        _ => {
            return Err(StorageError::InvalidInput(
                "invalid source observation category".to_owned(),
            ));
        }
    };
    let query = format!(
        "SELECT EXISTS(SELECT 1 FROM {table}
         WHERE file_version_id = ?1 AND observation_id = ?2)"
    );
    connection
        .query_row(&query, params![file_version_id, observation_id], |row| {
            row.get::<_, bool>(0)
        })
        .map_err(StorageError::from)
}

fn complete_generation(
    connection: &mut Connection,
    completion: &GenerationCompletion,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    ensure_building_generation(&transaction, &completion.generation_id)?;
    let now = now_unix()?;
    transaction.execute(
        "UPDATE generations SET scan_complete = ?2, coverage_status = ?3,
            warning_count = ?4, failure_summary = ?5, completed_at = ?6
         WHERE id = ?1",
        params![
            completion.generation_id.as_str(),
            if completion.scan_complete {
                1_i64
            } else {
                0_i64
            },
            completion.coverage_status,
            as_i64(completion.warning_count, "warning_count")?,
            completion.failure_summary,
            now,
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn ensure_building_generation(
    transaction: &Transaction<'_>,
    generation_id: &GenerationId,
) -> Result<(), StorageError> {
    let status: Option<String> = transaction
        .query_row(
            "SELECT status FROM generations WHERE id = ?1",
            [generation_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if status.as_deref() != Some("building") {
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "generation is missing or is not building",
        });
    }
    Ok(())
}

fn stage_file(
    transaction: &Transaction<'_>,
    generation_id: &str,
    file: &StagedFile,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO files(relative_path) VALUES (?1)
         ON CONFLICT(relative_path) DO NOTHING",
        [&file.relative_path],
    )?;
    let file_id: i64 = transaction.query_row(
        "SELECT id FROM files WHERE relative_path = ?1",
        [&file.relative_path],
        |row| row.get(0),
    )?;
    let byte_length = i64::try_from(file.byte_length)
        .map_err(|_| StorageError::InvalidInput("file length exceeds SQLite INTEGER".to_owned()))?;
    let config_hash: String = transaction.query_row(
        "SELECT config_hash FROM generations WHERE id = ?1",
        [generation_id],
        |row| row.get(0),
    )?;
    let mut analysis_key = String::from("analysis-v1:");
    for part in [
        &file.language,
        &file.grammar_hash,
        &file.query_hash,
        &file.extractor_hash,
        &config_hash,
    ] {
        analysis_key.push_str(&format!("{}:{part}", part.len()));
    }
    let inserted = transaction.execute(
        "INSERT INTO file_versions(
            file_id, content_hash, analysis_key, byte_length, source_encoding,
            language, grammar_hash, query_hash, parse_status, coverage_json, extractor_hash, config_hash
         ) VALUES (?1, ?2, ?3, ?4, 'utf-8', ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(file_id, content_hash, analysis_key) DO NOTHING",
        params![
            file_id,
            file.content_hash,
            analysis_key,
            byte_length,
            file.language,
            file.grammar_hash,
            file.query_hash,
            file.parse_status,
            file.coverage_json,
            file.extractor_hash,
            config_hash,
        ],
    )?;
    let file_version_id: i64 = transaction.query_row(
        "SELECT id FROM file_versions
         WHERE file_id = ?1 AND content_hash = ?2 AND analysis_key = ?3",
        params![file_id, file.content_hash, analysis_key],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO generation_files(generation_id, file_id, file_version_id)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(generation_id, file_id) DO UPDATE SET file_version_id = excluded.file_version_id",
        params![generation_id, file_id, file_version_id],
    )?;
    if inserted == 0 {
        return Ok(());
    }
    for fact in &file.facts {
        transaction.execute(
            "INSERT INTO facts(id, file_version_id, kind, name, start_byte, end_byte)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(file_version_id, id) DO NOTHING",
            params![
                fact.id,
                file_version_id,
                fact.kind,
                fact.name,
                i64::try_from(fact.byte_range.start()).map_err(|_| {
                    StorageError::InvalidInput("fact start exceeds SQLite INTEGER".to_owned())
                })?,
                i64::try_from(fact.byte_range.end()).map_err(|_| {
                    StorageError::InvalidInput("fact end exceeds SQLite INTEGER".to_owned())
                })?
            ],
        )?;
    }
    for observation in &file.observations {
        let statement = format!(
            "INSERT INTO {}(
                file_version_id, observation_id, kind, spelling,
                start_byte, end_byte, syntax_start_byte, syntax_end_byte,
                scope_id, container, signature, receiver, alias, target_id,
                resolution, attributes_json, limitations_json
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17
             ) ON CONFLICT(file_version_id, observation_id) DO NOTHING",
            observation.category.table_name()
        );
        transaction.execute(
            &statement,
            params![
                file_version_id,
                observation.id,
                observation.kind,
                observation.spelling,
                range_start(observation.byte_range, "observation start")?,
                range_end(observation.byte_range, "observation end")?,
                range_start(observation.syntax_range, "syntax start")?,
                range_end(observation.syntax_range, "syntax end")?,
                observation.scope_id,
                observation.container,
                observation.signature,
                observation.receiver,
                observation.alias,
                observation.target_id,
                observation.resolution,
                observation.attributes_json,
                observation.limitations_json,
            ],
        )?;
    }
    for (ordinal, diagnostic) in file.diagnostics.iter().enumerate() {
        let ordinal = i64::try_from(ordinal).map_err(|_| {
            StorageError::InvalidInput("diagnostic ordinal exceeds SQLite INTEGER".to_owned())
        })?;
        let start = diagnostic
            .byte_range
            .map(|range| range_start(range, "diagnostic start"))
            .transpose()?;
        let end = diagnostic
            .byte_range
            .map(|range| range_end(range, "diagnostic end"))
            .transpose()?;
        transaction.execute(
            "INSERT INTO diagnostics(
                file_version_id, ordinal, code, message, severity, start_byte, end_byte
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(file_version_id, ordinal) DO NOTHING",
            params![
                file_version_id,
                ordinal,
                diagnostic.code,
                diagnostic.message,
                diagnostic.severity,
                start,
                end,
            ],
        )?;
    }
    for document in &file.search_documents {
        transaction.execute(
            "INSERT INTO search_documents(file_version_id, name, path, content)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(file_version_id, name, content) DO NOTHING",
            params![
                file_version_id,
                document.name,
                file.relative_path,
                document.content
            ],
        )?;
    }
    Ok(())
}

fn as_i64(value: u64, field: &str) -> Result<i64, StorageError> {
    i64::try_from(value)
        .map_err(|_| StorageError::InvalidInput(format!("{field} exceeds SQLite INTEGER")))
}

fn range_start(range: ByteRange, field: &str) -> Result<i64, StorageError> {
    as_i64(range.start(), field)
}

fn range_end(range: ByteRange, field: &str) -> Result<i64, StorageError> {
    as_i64(range.end(), field)
}

fn read_job(connection: &Connection, job_id: &str) -> Result<Option<JobRecord>, StorageError> {
    connection
        .query_row(
            "SELECT id, generation_id, mode, request_key, status, cancel_requested,
                    files_discovered, files_reused, files_parsed, files_failed,
                    files_persisted, files_deleted, warning_count, error_summary,
                    created_at, updated_at
             FROM jobs WHERE id = ?1",
            [job_id],
            job_from_row,
        )
        .optional()
        .map_err(StorageError::from)
}

fn read_job_by_request_key(
    connection: &Connection,
    request_key: &str,
) -> Result<Option<JobRecord>, StorageError> {
    connection
        .query_row(
            "SELECT id, generation_id, mode, request_key, status, cancel_requested,
                    files_discovered, files_reused, files_parsed, files_failed,
                    files_persisted, files_deleted, warning_count, error_summary,
                    created_at, updated_at
             FROM jobs WHERE operation = 'index' AND request_key = ?1",
            [request_key],
            job_from_row,
        )
        .optional()
        .map_err(StorageError::from)
}

fn read_generation_resolution_files(
    connection: &Connection,
    generation_id: &GenerationId,
) -> Result<Vec<StoredResolutionFile>, StorageError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM generations WHERE id = ?1)",
        [generation_id.as_str()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "generation does not exist",
        });
    }
    let mut files = BTreeMap::new();
    let mut statement = connection.prepare(
        "SELECT files.relative_path, fv.language
         FROM generation_files AS gf
         JOIN files ON files.id = gf.file_id
         JOIN file_versions AS fv ON fv.id = gf.file_version_id
         WHERE gf.generation_id = ?1
         ORDER BY files.relative_path",
    )?;
    let rows = statement.query_map([generation_id.as_str()], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (relative_path, language) = row?;
        files.insert(
            relative_path.clone(),
            StoredResolutionFile {
                relative_path,
                language,
                observations: Vec::new(),
            },
        );
    }
    for (category, table) in [
        (ObservationCategory::Symbol, "symbols"),
        (ObservationCategory::Scope, "scopes"),
        (ObservationCategory::Import, "imports"),
        (ObservationCategory::Reference, "\"references\""),
        (ObservationCategory::CallSite, "call_sites"),
    ] {
        let sql = format!(
            "SELECT files.relative_path, item.observation_id, item.kind, item.spelling,
                    item.start_byte, item.end_byte, item.syntax_start_byte,
                    item.syntax_end_byte, item.scope_id, item.container, item.signature,
                    item.receiver, item.alias, item.target_id, item.resolution,
                    item.attributes_json, item.limitations_json
             FROM generation_files AS gf
             JOIN files ON files.id = gf.file_id
             JOIN {table} AS item ON item.file_version_id = gf.file_version_id
             WHERE gf.generation_id = ?1
             ORDER BY files.relative_path, item.start_byte, item.observation_id"
        );
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query([generation_id.as_str()])?;
        while let Some(row) = rows.next()? {
            let relative_path: String = row.get(0)?;
            let byte_range = stored_range(row.get(4)?, row.get(5)?, "observation")?;
            let syntax_range = stored_range(row.get(6)?, row.get(7)?, "syntax")?;
            let observation = StoredObservation {
                category,
                id: row.get(1)?,
                kind: row.get(2)?,
                spelling: row.get(3)?,
                byte_range,
                syntax_range,
                scope_id: row.get(8)?,
                container: row.get(9)?,
                signature: row.get(10)?,
                receiver: row.get(11)?,
                alias: row.get(12)?,
                target_id: row.get(13)?,
                resolution: row.get(14)?,
                attributes_json: row.get(15)?,
                limitations_json: row.get(16)?,
            };
            files
                .get_mut(&relative_path)
                .ok_or_else(|| {
                    StorageError::InvalidInput(
                        "resolution observation has no generation file membership".to_owned(),
                    )
                })?
                .observations
                .push(observation);
        }
    }
    Ok(files.into_values().collect())
}

fn stored_range(start: i64, end: i64, field: &str) -> Result<ByteRange, StorageError> {
    let start = u64::try_from(start)
        .map_err(|_| StorageError::InvalidInput(format!("negative stored {field} start")))?;
    let end = u64::try_from(end)
        .map_err(|_| StorageError::InvalidInput(format!("negative stored {field} end")))?;
    ByteRange::new(start, end).map_err(|error| StorageError::InvalidInput(error.to_string()))
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let id: String = row.get(0)?;
    let generation_id: Option<String> = row.get(1)?;
    let state: String = row.get(4)?;
    let convert_error = |message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
        )
    };
    let id = JobId::new(id).map_err(|error| convert_error(error.to_string()))?;
    let generation_id = generation_id
        .map(GenerationId::new)
        .transpose()
        .map_err(|error| convert_error(error.to_string()))?;
    let state = JobState::from_str(&state).map_err(|error| convert_error(error.to_string()))?;
    let unsigned = |index| -> rusqlite::Result<u64> {
        let value: i64 = row.get(index)?;
        u64::try_from(value).map_err(|_| convert_error("negative job counter".to_owned()))
    };
    Ok(JobRecord {
        id,
        generation_id,
        mode: row.get(2)?,
        request_key: row.get(3)?,
        state,
        cancel_requested: row.get::<_, i64>(5)? != 0,
        progress: JobProgress {
            files_discovered: unsigned(6)?,
            files_reused: unsigned(7)?,
            files_parsed: unsigned(8)?,
            files_failed: unsigned(9)?,
            files_persisted: unsigned(10)?,
            files_deleted: unsigned(11)?,
            warning_count: unsigned(12)?,
        },
        error_summary: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn read_index_summary(connection: &Connection) -> Result<IndexSummary, StorageError> {
    let active: Option<String> = connection.query_row(
        "SELECT active_generation_id FROM meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    let active_generation_id = active
        .as_deref()
        .map(GenerationId::new)
        .transpose()
        .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
    let count_for = |table: &str| -> Result<u64, StorageError> {
        let Some(active) = active.as_deref() else {
            return Ok(0);
        };
        let sql = match table {
            "generation_files" => {
                "SELECT COUNT(*) FROM generation_files WHERE generation_id = ?1".to_owned()
            }
            "facts" | "symbols" => format!(
                "SELECT COUNT(*) FROM {table} AS item
                 JOIN generation_files AS gf ON gf.file_version_id = item.file_version_id
                 WHERE gf.generation_id = ?1"
            ),
            _ => {
                return Err(StorageError::InvalidInput(
                    "invalid internal summary table".to_owned(),
                ));
            }
        };
        let count: i64 = connection.query_row(&sql, [active], |row| row.get(0))?;
        u64::try_from(count)
            .map_err(|_| StorageError::InvalidInput("negative summary count".to_owned()))
    };
    let latest_job = connection
        .query_row(
            "SELECT id, generation_id, mode, request_key, status, cancel_requested,
                    files_discovered, files_reused, files_parsed, files_failed,
                    files_persisted, files_deleted, warning_count, error_summary,
                    created_at, updated_at
             FROM jobs ORDER BY created_at DESC, id DESC LIMIT 1",
            [],
            job_from_row,
        )
        .optional()?;
    Ok(IndexSummary {
        active_generation_id,
        active_files: count_for("generation_files")?,
        active_facts: count_for("facts")?,
        active_symbols: count_for("symbols")?,
        latest_job,
    })
}

fn activate_generation(
    connection: &mut Connection,
    generation_id: &GenerationId,
) -> Result<(), StorageError> {
    let now = now_unix()?;
    let transaction = connection.transaction()?;
    let generation: Option<(String, i64, String)> = transaction
        .query_row(
            "SELECT status, scan_complete, resolver_version FROM generations WHERE id = ?1",
            [generation_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((status, scan_complete, resolver_version)) = generation else {
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "generation does not exist",
        });
    };
    if status != "building" {
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "generation is not in the building state",
        });
    }
    if scan_complete != 1 {
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "repository scan is incomplete",
        });
    }
    if resolver_version == "none" {
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "candidate generation has not completed graph resolution",
        });
    }
    let job_status: Option<String> = transaction
        .query_row(
            "SELECT status FROM jobs WHERE generation_id = ?1",
            [generation_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if job_status.as_deref() != Some(JobState::Committing.as_str()) {
        return Err(StorageError::InvalidGeneration {
            generation: generation_id.as_str().to_owned(),
            reason: "generation job is not in the committing state",
        });
    }

    transaction.execute(
        "UPDATE generations SET status = 'superseded' WHERE status = 'active'",
        [],
    )?;
    transaction.execute(
        "UPDATE generations SET status = 'active', activated_at = ?2 WHERE id = ?1",
        params![generation_id.as_str(), now],
    )?;
    transaction.execute(
        "UPDATE meta SET active_generation_id = ?1, updated_at = ?2 WHERE singleton = 1",
        params![generation_id.as_str(), now],
    )?;
    transaction.execute(
        "UPDATE jobs SET status = 'completed', updated_at = ?2
         WHERE generation_id = ?1",
        params![generation_id.as_str(), now],
    )?;
    transaction.commit()?;
    Ok(())
}

fn abandon_generation(
    connection: &mut Connection,
    generation_id: Option<&GenerationId>,
    job_id: &JobId,
    state: JobState,
    progress: &JobProgress,
    error_summary: &str,
) -> Result<(), StorageError> {
    let now = now_unix()?;
    let transaction = connection.transaction()?;
    let job = read_job(&transaction, job_id.as_str())?
        .ok_or_else(|| StorageError::JobNotFound(job_id.as_str().to_owned()))?;
    validate_job_transition(job.state, state).map_err(|()| StorageError::InvalidJobTransition {
        job: job_id.as_str().to_owned(),
        from: job.state.as_str().to_owned(),
        to: state.as_str().to_owned(),
    })?;
    if let Some(generation_id) = generation_id {
        transaction.execute(
            "UPDATE generations SET status = 'abandoned', scan_complete = 0,
                coverage_status = 'failed', failure_summary = ?2, completed_at = ?3
             WHERE id = ?1 AND status = 'building'",
            params![generation_id.as_str(), error_summary, now],
        )?;
    }
    transaction.execute(
        "UPDATE jobs SET status = ?2, error_summary = ?3,
            files_discovered = ?4, files_reused = ?5, files_parsed = ?6,
            files_failed = ?7, files_persisted = ?8, files_deleted = ?9,
            warning_count = ?10, updated_at = ?11
         WHERE id = ?1",
        params![
            job_id.as_str(),
            state.as_str(),
            error_summary,
            as_i64(progress.files_discovered, "files_discovered")?,
            as_i64(progress.files_reused, "files_reused")?,
            as_i64(progress.files_parsed, "files_parsed")?,
            as_i64(progress.files_failed, "files_failed")?,
            as_i64(progress.files_persisted, "files_persisted")?,
            as_i64(progress.files_deleted, "files_deleted")?,
            as_i64(progress.warning_count, "warning_count")?,
            now,
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn gc_abandoned(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    // Keep active plus its immediate predecessor. Detach historical parent links
    // before collection; memories are independent of generation lifetime.
    transaction.execute("UPDATE generations SET parent_id = NULL WHERE parent_id IN (
        SELECT id FROM generations WHERE status = 'superseded' AND id NOT IN (
            SELECT parent_id FROM generations WHERE status IN ('active', 'building') AND parent_id IS NOT NULL
        ))", [])?;
    transaction.execute("DELETE FROM generations WHERE status = 'superseded' AND id NOT IN (
        SELECT parent_id FROM generations WHERE status IN ('active', 'building') AND parent_id IS NOT NULL
    )", [])?;
    transaction.execute(
        "DELETE FROM generations
         WHERE status = 'abandoned'
           AND id <> COALESCE((SELECT active_generation_id FROM meta WHERE singleton = 1), '')",
        [],
    )?;
    transaction.execute(
        "DELETE FROM file_versions
         WHERE NOT EXISTS (
             SELECT 1 FROM generation_files WHERE file_version_id = file_versions.id
         )",
        [],
    )?;
    transaction.execute(
        "DELETE FROM files
         WHERE NOT EXISTS (
             SELECT 1 FROM file_versions WHERE file_id = files.id
         )",
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

fn put_memory(connection: &mut Connection, id: &str, body: &str) -> Result<(), StorageError> {
    let now = now_unix()?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO memories(id, body, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET body = excluded.body, updated_at = excluded.updated_at",
        params![id, body, now],
    )?;
    transaction.commit()?;
    Ok(())
}

fn upsert_memory(
    connection: &mut Connection,
    repository_id: &str,
    draft: &MemoryDraft,
) -> Result<MemoryRecord, StorageError> {
    let id = match draft.memory_id.as_deref() {
        Some(id) => id.to_owned(),
        None => {
            let counter = NEXT_MEMORY_ID.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| {
                    StorageError::InvalidInput("system clock is before Unix epoch".to_owned())
                })?
                .as_nanos();
            format!("mem-{nanos:032x}-{counter:016x}")
        }
    };
    validate_memory_id(&id)?;
    let now = now_unix()?;
    let transaction = connection.transaction()?;
    let existing_revision = transaction
        .query_row(
            "SELECT revision FROM memories WHERE id = ?1",
            [&id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                StorageError::InvalidInput("stored memory revision is negative".to_owned())
            })
        })
        .transpose()?;
    let revision = match (existing_revision, draft.expected_revision) {
        (Some(_), None) => {
            return Err(StorageError::MemoryRevisionRequired { id });
        }
        (Some(actual), Some(expected)) if actual != expected => {
            return Err(StorageError::MemoryRevisionConflict {
                id,
                expected,
                actual,
            });
        }
        (Some(actual), Some(_)) => actual
            .checked_add(1)
            .ok_or_else(|| StorageError::InvalidInput("memory revision overflow".to_owned()))?,
        (None, Some(expected)) => {
            return Err(StorageError::MemoryRevisionConflict {
                id,
                expected,
                actual: 0,
            });
        }
        (None, None) => {
            let count: i64 =
                transaction.query_row("SELECT count(*) FROM memories", [], |row| row.get(0))?;
            if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_MEMORIES {
                return Err(StorageError::MemoryLimitExceeded {
                    limit: MAX_MEMORIES,
                });
            }
            1
        }
    };
    let revision_sql = i64::try_from(revision).map_err(|_| {
        StorageError::InvalidInput("memory revision exceeds SQLite range".to_owned())
    })?;
    validate_memory_evidence(&transaction, &draft.evidence)?;
    transaction.execute(
        "INSERT INTO memories(
             id, body, created_at, updated_at, kind, revision, author, origin, scope
         ) VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
             body = excluded.body,
             updated_at = excluded.updated_at,
             kind = excluded.kind,
             revision = excluded.revision,
             author = excluded.author,
             origin = excluded.origin,
             scope = excluded.scope",
        params![
            id,
            draft.text,
            now,
            draft.kind.as_str(),
            revision_sql,
            draft.author,
            draft.origin,
            draft.scope,
        ],
    )?;
    transaction.execute("DELETE FROM memory_evidence WHERE memory_id = ?1", [&id])?;
    for evidence in &draft.evidence {
        transaction.execute(
            "INSERT INTO memory_evidence(
                 memory_id, repository_id, relative_path, content_hash, symbol_id
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                repository_id,
                evidence.relative_path,
                evidence.content_hash,
                evidence.symbol_id,
            ],
        )?;
    }
    let record = read_memory_record(&transaction, repository_id, &id)?.ok_or_else(|| {
        StorageError::InvalidInput("newly written memory could not be read".to_owned())
    })?;
    transaction.commit()?;
    Ok(record)
}

fn forget_memory(
    connection: &mut Connection,
    repository_id: &str,
    id: &str,
    expected_revision: u64,
) -> Result<MemoryRecord, StorageError> {
    let transaction = connection.transaction()?;
    let record = read_memory_record(&transaction, repository_id, id)?
        .ok_or_else(|| StorageError::MemoryNotFound(id.to_owned()))?;
    if record.revision != expected_revision {
        return Err(StorageError::MemoryRevisionConflict {
            id: id.to_owned(),
            expected: expected_revision,
            actual: record.revision,
        });
    }
    transaction.execute("DELETE FROM memories WHERE id = ?1", [id])?;
    transaction.commit()?;
    Ok(record)
}

fn validate_memory_evidence(
    connection: &Connection,
    evidence: &[MemoryEvidence],
) -> Result<(), StorageError> {
    for item in evidence {
        let valid: bool = connection.query_row(
            "SELECT EXISTS(
                 SELECT 1
                 FROM meta
                 JOIN generation_files AS gf ON gf.generation_id = meta.active_generation_id
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 WHERE meta.singleton = 1
                   AND files.relative_path = ?1
                   AND fv.content_hash = ?2
                   AND (?3 IS NULL OR EXISTS (
                       SELECT 1 FROM symbols AS s
                       WHERE s.file_version_id = fv.id AND s.observation_id = ?3
                   ))
             )",
            params![item.relative_path, item.content_hash, item.symbol_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(StorageError::MemoryEvidenceInvalid(format!(
                "{}{}",
                item.relative_path,
                item.symbol_id
                    .as_deref()
                    .map_or_else(String::new, |symbol| format!("#{symbol}"))
            )));
        }
    }
    Ok(())
}

fn read_memory_record(
    connection: &Connection,
    repository_id: &str,
    id: &str,
) -> Result<Option<MemoryRecord>, StorageError> {
    let base = connection
        .query_row(
            "SELECT body, kind, revision, author, origin, scope, created_at, updated_at
             FROM memories WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((text, kind, revision, author, origin, scope, created_at, updated_at)) = base else {
        return Ok(None);
    };
    let mut statement = connection.prepare(
        "SELECT relative_path, content_hash, symbol_id
         FROM memory_evidence
         WHERE memory_id = ?1 AND repository_id = ?2
         ORDER BY relative_path, content_hash, COALESCE(symbol_id, '')",
    )?;
    let rows = statement.query_map(params![id, repository_id], |row| {
        Ok(MemoryEvidence {
            relative_path: row.get(0)?,
            content_hash: row.get(1)?,
            symbol_id: row.get(2)?,
        })
    })?;
    let evidence = rows.collect::<Result<Vec<_>, _>>()?;
    let evidence_status = memory_evidence_status(connection, &evidence)?;
    Ok(Some(MemoryRecord {
        id: id.to_owned(),
        text,
        kind: MemoryKind::parse(&kind)?,
        revision: u64::try_from(revision).map_err(|_| {
            StorageError::InvalidInput("stored memory revision is negative".to_owned())
        })?,
        author,
        origin,
        scope,
        created_at,
        updated_at,
        evidence,
        evidence_status,
    }))
}

fn memory_evidence_status(
    connection: &Connection,
    evidence: &[MemoryEvidence],
) -> Result<MemoryEvidenceStatus, StorageError> {
    if evidence.is_empty() {
        return Ok(MemoryEvidenceStatus::Unverified);
    }
    let active: Option<String> = connection
        .query_row(
            "SELECT active_generation_id FROM meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if active.is_none() {
        return Ok(MemoryEvidenceStatus::Unverified);
    }
    for item in evidence {
        let valid: bool = connection.query_row(
            "SELECT EXISTS(
                 SELECT 1
                 FROM meta
                 JOIN generation_files AS gf ON gf.generation_id = meta.active_generation_id
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 WHERE meta.singleton = 1
                   AND files.relative_path = ?1
                   AND fv.content_hash = ?2
                   AND (?3 IS NULL OR EXISTS (
                       SELECT 1 FROM symbols AS s
                       WHERE s.file_version_id = fv.id AND s.observation_id = ?3
                   ))
             )",
            params![item.relative_path, item.content_hash, item.symbol_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Ok(MemoryEvidenceStatus::Stale);
        }
    }
    Ok(MemoryEvidenceStatus::Verified)
}

fn search_memory_records(
    connection: &Connection,
    repository_id: &str,
    request: &MemorySearch,
) -> Result<Vec<MemoryRecord>, StorageError> {
    let literal = format!("\"{}\"", request.query.replace('"', "\"\""));
    let fetch_limit = i64::try_from(MAX_MEMORIES)
        .map_err(|_| StorageError::InvalidInput("memory search limit overflow".to_owned()))?;
    let mut statement = connection.prepare(
        "SELECT memories.id
         FROM memory_fts
         JOIN memories ON memories.rowid = memory_fts.rowid
         WHERE memory_fts MATCH ?1 AND (?2 IS NULL OR memories.scope = ?2)
         ORDER BY bm25(memory_fts), memories.updated_at DESC, memories.id
         LIMIT ?3",
    )?;
    let rows = statement.query_map(params![literal, request.scope, fetch_limit], |row| {
        row.get::<_, String>(0)
    })?;
    let ids = rows.collect::<Result<Vec<_>, _>>()?;
    let mut output = Vec::new();
    for id in ids {
        let Some(record) = read_memory_record(connection, repository_id, &id)? else {
            continue;
        };
        if !request.include_stale && record.evidence_status == MemoryEvidenceStatus::Stale {
            continue;
        }
        output.push(record);
        if output.len() == request.limit {
            break;
        }
    }
    Ok(output)
}

fn backup_database(connection: &Connection, destination: &Path) -> Result<(), StorageError> {
    if destination.exists() {
        return Err(StorageError::BackupExists(destination.to_owned()));
    }
    let parent = destination.parent().ok_or_else(|| {
        StorageError::InvalidInput("backup destination has no parent directory".to_owned())
    })?;
    if !parent.is_dir() {
        return Err(StorageError::InvalidInput(format!(
            "backup parent is not an existing directory: {}",
            parent.display()
        )));
    }

    let mut reserve_options = OpenOptions::new();
    reserve_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        reserve_options.mode(0o600);
    }
    let reserved = reserve_options
        .open(destination)
        .map_err(|source| io_error(destination, source))?;
    drop(reserved);
    if let Err(error) = set_file_permissions(destination) {
        let _ = fs::remove_file(destination);
        return Err(error);
    }

    let result = (|| {
        let mut target = Connection::open_with_flags(
            destination,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        let backup = Backup::new(connection, &mut target)?;
        backup.run_to_completion(128, Duration::from_millis(10), None)?;
        drop(backup);
        let integrity: String = target.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(StorageError::InvalidInput(format!(
                "backup integrity check failed: {integrity}"
            )));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn now_unix() -> Result<i64, StorageError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StorageError::InvalidInput("system clock predates Unix epoch".to_owned()))?;
    i64::try_from(duration.as_secs())
        .map_err(|_| StorageError::InvalidInput("system clock exceeds SQLite INTEGER".to_owned()))
}

pub struct ReadSnapshot {
    connection: Connection,
    repository_id: RepositoryId,
    generation_id: GenerationId,
}

impl ReadSnapshot {
    fn open(database_path: &Path, repository_id: &str) -> Result<Self, StorageError> {
        if !database_path.is_file() {
            return Err(StorageError::IndexNotReady);
        }
        let connection = open_reader_connection(database_path)?;
        validate_schema_and_root(&connection, repository_id)?;
        connection.execute_batch("BEGIN DEFERRED")?;
        let active: Option<String> = connection.query_row(
            "SELECT active_generation_id FROM meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        let active = active.ok_or(StorageError::IndexNotReady)?;
        let generation_id = GenerationId::new(active)
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let repository_id = RepositoryId::new(repository_id.to_owned())
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        Ok(Self {
            connection,
            repository_id,
            generation_id,
        })
    }

    #[must_use]
    pub fn repository_id(&self) -> &RepositoryId {
        &self.repository_id
    }

    #[must_use]
    pub fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    pub fn facts(&self) -> Result<Vec<SnapshotFact>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT f.id, files.relative_path, f.kind, f.name, f.start_byte, f.end_byte
             FROM generation_files AS gf
             JOIN files ON files.id = gf.file_id
             JOIN facts AS f ON f.file_version_id = gf.file_version_id
             WHERE gf.generation_id = ?1
             ORDER BY files.relative_path, f.start_byte, f.id",
        )?;
        let rows = statement.query_map([self.generation_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;
        let mut facts = Vec::new();
        for row in rows {
            let (id, relative_path, kind, name, start_byte, end_byte) = row?;
            facts.push(SnapshotFact {
                id,
                relative_path,
                kind,
                name,
                start_byte: u64::try_from(start_byte).map_err(|_| {
                    StorageError::InvalidInput("negative stored fact start".to_owned())
                })?,
                end_byte: u64::try_from(end_byte).map_err(|_| {
                    StorageError::InvalidInput("negative stored fact end".to_owned())
                })?,
            });
        }
        Ok(facts)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, StorageError> {
        if query.is_empty() || query.len() > MAX_SEARCH_QUERY_BYTES || query.contains('\0') {
            return Err(StorageError::InvalidInput(
                "search query is empty or exceeds policy".to_owned(),
            ));
        }
        if !(1..=100).contains(&limit) {
            return Err(StorageError::InvalidInput(
                "search limit must be in 1..=100".to_owned(),
            ));
        }
        let literal = format!("\"{}\"", query.replace('"', "\"\""));
        let limit = i64::try_from(limit)
            .map_err(|_| StorageError::InvalidInput("search limit overflow".to_owned()))?;
        let mut statement = self.connection.prepare(
            "SELECT sd.name, sd.path
             FROM symbol_fts
             JOIN search_documents AS sd ON sd.id = symbol_fts.rowid
             JOIN generation_files AS gf ON gf.file_version_id = sd.file_version_id
             WHERE symbol_fts MATCH ?1 AND gf.generation_id = ?2
             ORDER BY bm25(symbol_fts), sd.path, sd.name
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![literal, self.generation_id.as_str(), limit],
            |row| {
                Ok(SearchHit {
                    name: row.get(0)?,
                    relative_path: row.get(1)?,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn retrieval_coverage(&self) -> Result<StoredCoverage, StorageError> {
        let (status, warning_count, unresolved_occurrences, resolver_version) =
            self.connection.query_row(
                "SELECT coverage_status, warning_count, unresolved_occurrence_count,
                        resolver_version
                 FROM generations WHERE id = ?1",
                [self.generation_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
        Ok(StoredCoverage {
            status,
            warning_count: nonnegative_u64(warning_count, "generation warning count")?,
            unresolved_occurrences: nonnegative_u64(
                unresolved_occurrences,
                "unresolved occurrence count",
            )?,
            resolver_version,
        })
    }

    pub fn retrieval_symbol(
        &self,
        symbol_id: &str,
    ) -> Result<Option<StoredSymbolRecord>, StorageError> {
        validate_retrieval_text(symbol_id, 512, "symbol ID")?;
        self.connection
            .query_row(
                "SELECT s.observation_id, files.relative_path, fv.language, s.kind,
                        s.spelling, s.container, s.signature, s.start_byte, s.end_byte,
                        s.syntax_start_byte, s.syntax_end_byte, fv.content_hash,
                        fv.parse_status, s.attributes_json, s.limitations_json
                 FROM generation_files AS gf
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 JOIN symbols AS s ON s.file_version_id = gf.file_version_id
                 WHERE gf.generation_id = ?1 AND s.observation_id = ?2
                 ORDER BY files.relative_path LIMIT 1",
                params![self.generation_id.as_str(), symbol_id],
                stored_symbol_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn retrieval_file_symbols(
        &self,
        relative_path: &str,
        limit: usize,
    ) -> Result<Vec<StoredSymbolRecord>, StorageError> {
        validate_retrieval_text(relative_path, 4_096, "relative path")?;
        validate_retrieval_limit(limit, 20_001)?;
        let limit = to_sql_limit(limit)?;
        let mut statement = self.connection.prepare(
            "SELECT s.observation_id, files.relative_path, fv.language, s.kind,
                    s.spelling, s.container, s.signature, s.start_byte, s.end_byte,
                    s.syntax_start_byte, s.syntax_end_byte, fv.content_hash,
                    fv.parse_status, s.attributes_json, s.limitations_json
             FROM generation_files AS gf
             JOIN files ON files.id = gf.file_id
             JOIN file_versions AS fv ON fv.id = gf.file_version_id
             JOIN symbols AS s ON s.file_version_id = gf.file_version_id
             WHERE gf.generation_id = ?1 AND files.relative_path = ?2
             ORDER BY s.start_byte, s.end_byte, s.observation_id LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![self.generation_id.as_str(), relative_path, limit],
            stored_symbol_from_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn retrieval_search_symbols(
        &self,
        plan: &StoredSearchPlan,
    ) -> Result<Vec<StoredSearchSymbol>, StorageError> {
        validate_search_plan(plan)?;
        let path_like = plan
            .path_prefix
            .as_deref()
            .map(|path| format!("{}/%", escape_sql_like(path)));
        let candidate_limit = to_sql_limit(plan.fetch_limit)?;
        let after = plan.after.as_ref();
        let after_tier = after.map(|key| key.tier);
        let after_score = after.map_or(0.0, |key| key.fts_score);
        let after_path = after.map_or("", |key| key.relative_path.as_str());
        let after_name = after.map_or("", |key| key.name.as_str());
        let after_start = as_i64(after.map_or(0, |key| key.start_byte), "cursor start")?;
        let after_id = after.map_or("", |key| key.symbol_id.as_str());
        let mut candidates = Vec::new();
        let mut direct = self.connection.prepare(
            "SELECT * FROM (
                 SELECT s.observation_id, files.relative_path, fv.language, s.kind,
                        s.spelling, s.container, s.signature, s.start_byte, s.end_byte,
                        s.syntax_start_byte, s.syntax_end_byte, fv.content_hash,
                        fv.parse_status, s.attributes_json, s.limitations_json, 1 AS tier
                 FROM symbols AS s INDEXED BY symbols_name_idx
                 JOIN generation_files AS gf ON gf.file_version_id = s.file_version_id
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 WHERE s.spelling = ?2 AND gf.generation_id = ?1
                   AND (?5 IS NULL OR fv.language = ?5)
                   AND (?6 IS NULL OR s.kind = ?6)
                   AND (?7 IS NULL OR files.relative_path = ?7
                        OR files.relative_path LIKE ?8 ESCAPE '\\')
                 UNION ALL
                 SELECT s.observation_id, files.relative_path, fv.language, s.kind,
                        s.spelling, s.container, s.signature, s.start_byte, s.end_byte,
                        s.syntax_start_byte, s.syntax_end_byte, fv.content_hash,
                        fv.parse_status, s.attributes_json, s.limitations_json, 2 AS tier
                 FROM symbols AS s INDEXED BY symbols_folded_spelling_idx
                 CROSS JOIN generation_files AS gf ON gf.file_version_id = s.file_version_id
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 WHERE lower(s.spelling) = ?3 AND gf.generation_id = ?1
                   AND s.spelling <> ?2
                   AND (?5 IS NULL OR fv.language = ?5)
                   AND (?6 IS NULL OR s.kind = ?6)
                   AND (?7 IS NULL OR files.relative_path = ?7
                        OR files.relative_path LIKE ?8 ESCAPE '\\')
                 UNION ALL
                 SELECT s.observation_id, files.relative_path, fv.language, s.kind,
                        s.spelling, s.container, s.signature, s.start_byte, s.end_byte,
                        s.syntax_start_byte, s.syntax_end_byte, fv.content_hash,
                        fv.parse_status, s.attributes_json, s.limitations_json,
                        CASE WHEN substr(s.spelling, 1, length(?2)) = ?2
                             THEN 3 ELSE 4 END AS tier
                 FROM symbols AS s INDEXED BY symbols_folded_spelling_idx
                 JOIN generation_files AS gf ON gf.file_version_id = s.file_version_id
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 WHERE lower(s.spelling) >= ?3
                   AND lower(s.spelling) <> ?3
                   AND lower(s.spelling) < (?3 || char(1114111))
                   AND lower(s.spelling) LIKE ?4 ESCAPE '\\'
                   AND gf.generation_id = ?1
                   AND (?5 IS NULL OR fv.language = ?5)
                   AND (?6 IS NULL OR s.kind = ?6)
                   AND (?7 IS NULL OR files.relative_path = ?7
                        OR files.relative_path LIKE ?8 ESCAPE '\\')
             ) AS ranked
             WHERE (?10 IS NULL OR (tier, 0.0, relative_path, spelling, start_byte, observation_id)
                    > (?10, ?11, ?12, ?13, ?14, ?15))
             ORDER BY tier, relative_path, spelling, start_byte, observation_id LIMIT ?9",
        )?;
        let rows = direct.query_map(
            params![
                self.generation_id.as_str(),
                plan.query,
                plan.folded_query,
                plan.escaped_folded_prefix,
                plan.language,
                plan.kind,
                plan.path_prefix,
                path_like,
                candidate_limit,
                after_tier,
                after_score,
                after_path,
                after_name,
                after_start,
                after_id,
            ],
            |row| {
                Ok(StoredSearchSymbol {
                    symbol: stored_symbol_from_row(row)?,
                    tier: row.get(15)?,
                    fts_score: 0.0,
                })
            },
        )?;
        for row in rows {
            candidates.push(row?);
        }
        if let Some(leaf) = qualified_name_leaf(&plan.query) {
            let mut qualified = self.connection.prepare(
                "SELECT s.observation_id, files.relative_path, fv.language, s.kind,
                        s.spelling, s.container, s.signature, s.start_byte, s.end_byte,
                        s.syntax_start_byte, s.syntax_end_byte, fv.content_hash,
                        fv.parse_status, s.attributes_json, s.limitations_json,
                        0 AS tier
                 FROM symbols AS s INDEXED BY symbols_name_idx
                 JOIN generation_files AS gf ON gf.file_version_id = s.file_version_id
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 WHERE s.spelling = ?1 AND gf.generation_id = ?2
                   AND ((COALESCE(s.container, '') || '.' || s.spelling = ?3)
                     OR (COALESCE(s.container, '') || '::' || s.spelling = ?3))
                   AND (?4 IS NULL OR fv.language = ?4)
                   AND (?5 IS NULL OR s.kind = ?5)
                   AND (?6 IS NULL OR files.relative_path = ?6
                        OR files.relative_path LIKE ?7 ESCAPE '\\')
                   AND (?9 IS NULL OR (0, 0.0, files.relative_path, s.spelling, s.start_byte, s.observation_id)
                        > (?9, ?10, ?11, ?12, ?13, ?14))
                 ORDER BY files.relative_path, s.spelling, s.start_byte,
                          s.observation_id LIMIT ?8",
            )?;
            let rows = qualified.query_map(
                params![
                    leaf,
                    self.generation_id.as_str(),
                    plan.query,
                    plan.language,
                    plan.kind,
                    plan.path_prefix,
                    path_like,
                    candidate_limit,
                    after_tier,
                    after_score,
                    after_path,
                    after_name,
                    after_start,
                    after_id,
                ],
                |row| {
                    Ok(StoredSearchSymbol {
                        symbol: stored_symbol_from_row(row)?,
                        tier: row.get(15)?,
                        fts_score: 0.0,
                    })
                },
            )?;
            for row in rows {
                candidates.push(row?);
            }
        }
        let mut full_text = self.connection.prepare(
            "WITH fts_matches AS MATERIALIZED (
                 SELECT sd.file_version_id, sd.name, bm25(symbol_fts) AS score
                 FROM symbol_fts JOIN search_documents AS sd ON sd.id = symbol_fts.rowid
                 JOIN generation_files AS gf ON gf.file_version_id = sd.file_version_id
                 WHERE symbol_fts MATCH ?1 AND gf.generation_id = ?2
                   AND lower(sd.name) NOT LIKE ?8 ESCAPE '\\'
             ), scores AS (
                 SELECT file_version_id, name, MIN(score) AS score FROM fts_matches
                 GROUP BY file_version_id, name
             )
             SELECT s.observation_id, files.relative_path, fv.language, s.kind,
                    s.spelling, s.container, s.signature, s.start_byte, s.end_byte,
                    s.syntax_start_byte, s.syntax_end_byte, fv.content_hash,
                    fv.parse_status, s.attributes_json, s.limitations_json,
                    scores.score
             FROM scores
             JOIN generation_files AS gf ON gf.file_version_id = scores.file_version_id
             JOIN files ON files.id = gf.file_id
             JOIN file_versions AS fv ON fv.id = gf.file_version_id
             JOIN symbols AS s INDEXED BY symbols_version_spelling_idx ON s.file_version_id = gf.file_version_id
                              AND s.spelling = scores.name
             WHERE gf.generation_id = ?2
               AND lower(s.spelling) NOT LIKE ?8 ESCAPE '\\'
               AND NOT ((COALESCE(s.container, '') || '.' || s.spelling = ?9)
                    OR (COALESCE(s.container, '') || '::' || s.spelling = ?9))
               AND (?3 IS NULL OR fv.language = ?3)
               AND (?4 IS NULL OR s.kind = ?4)
               AND (?5 IS NULL OR files.relative_path = ?5
                    OR files.relative_path LIKE ?6 ESCAPE '\\')
               AND (?10 IS NULL OR (5, scores.score, files.relative_path, s.spelling, s.start_byte, s.observation_id)
                    > (?10, ?11, ?12, ?13, ?14, ?15))
             ORDER BY score, files.relative_path, s.spelling, s.start_byte,
                      s.observation_id LIMIT ?7",
        )?;
        let rows = full_text.query_map(
            params![
                plan.fts_expression,
                self.generation_id.as_str(),
                plan.language,
                plan.kind,
                plan.path_prefix,
                path_like,
                candidate_limit,
                plan.escaped_folded_prefix,
                plan.query,
                after_tier,
                after_score,
                after_path,
                after_name,
                after_start,
                after_id,
            ],
            |row| {
                Ok(StoredSearchSymbol {
                    symbol: stored_symbol_from_row(row)?,
                    tier: 5,
                    fts_score: row.get(15)?,
                })
            },
        )?;
        for row in rows {
            candidates.push(row?);
        }
        candidates.sort_by(stored_search_order);
        let mut seen = HashSet::new();
        candidates.retain(|candidate| {
            seen.insert((
                candidate.symbol.relative_path.clone(),
                candidate.symbol.id.clone(),
            ))
        });
        if let Some(after) = &plan.after {
            candidates.retain(|candidate| stored_search_after(candidate, after));
        }
        candidates.truncate(plan.fetch_limit);
        Ok(candidates)
    }

    pub fn retrieval_repository_files(
        &self,
        path_prefix: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredIndexedFile>, StorageError> {
        if let Some(path) = path_prefix {
            validate_retrieval_text(path, 4_096, "path prefix")?;
        }
        validate_retrieval_limit(limit, 5_001)?;
        let path_like = path_prefix.map(|path| format!("{}/%", escape_sql_like(path)));
        let mut statement = self.connection.prepare(
            "SELECT files.relative_path, fv.content_hash, fv.language, fv.parse_status,
                    fv.coverage_json, fv.byte_length, COUNT(s.observation_id)
             FROM generation_files AS gf
             JOIN files ON files.id = gf.file_id
             JOIN file_versions AS fv ON fv.id = gf.file_version_id
             LEFT JOIN symbols AS s ON s.file_version_id = gf.file_version_id
             WHERE gf.generation_id = ?1
               AND (?2 IS NULL OR files.relative_path = ?2
                    OR files.relative_path LIKE ?3 ESCAPE '\\')
             GROUP BY files.relative_path, fv.id
             ORDER BY files.relative_path LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![
                self.generation_id.as_str(),
                path_prefix,
                path_like,
                to_sql_limit(limit)?,
            ],
            stored_indexed_file_from_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn retrieval_indexed_file(
        &self,
        relative_path: &str,
    ) -> Result<Option<StoredIndexedFile>, StorageError> {
        validate_retrieval_text(relative_path, 4_096, "relative path")?;
        self.connection
            .query_row(
                "SELECT files.relative_path, fv.content_hash, fv.language, fv.parse_status,
                        fv.coverage_json, fv.byte_length, COUNT(s.observation_id)
                 FROM generation_files AS gf
                 JOIN files ON files.id = gf.file_id
                 JOIN file_versions AS fv ON fv.id = gf.file_version_id
                 LEFT JOIN symbols AS s ON s.file_version_id = gf.file_version_id
                 WHERE gf.generation_id = ?1 AND files.relative_path = ?2
                 GROUP BY files.relative_path, fv.id",
                params![self.generation_id.as_str(), relative_path],
                stored_indexed_file_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn retrieval_references_to(
        &self,
        symbol_id: &str,
        include_candidates: bool,
        after: Option<&StoredReferenceAfter>,
        limit: usize,
    ) -> Result<Vec<StoredReferenceRecord>, StorageError> {
        validate_retrieval_text(symbol_id, 512, "symbol ID")?;
        validate_retrieval_limit(limit, 201)?;
        let after_path = after.map(|key| key.relative_path.as_str());
        let after_start = after.map(|key| key.start_byte).unwrap_or_default();
        let after_edge = after.map(|key| key.edge_id.as_str());
        let after_start = i64::try_from(after_start)
            .map_err(|_| StorageError::InvalidInput("reference cursor overflow".to_owned()))?;
        let mut statement = self.connection.prepare(
            "SELECT re.edge_id, re.source_observation_id, source_file.relative_path,
                    COALESCE(source_reference.spelling, source_call.spelling,
                             source_symbol.spelling, ''),
                    re.source_start_byte, re.source_end_byte, re.resolution,
                    re.candidate_count, re.rule_version, re.limitations_json
             FROM resolved_edges AS re
             JOIN file_versions AS source_version
               ON source_version.id = re.source_file_version_id
             JOIN files AS source_file ON source_file.id = source_version.file_id
             LEFT JOIN \"references\" AS source_reference
               ON re.source_category = 'reference'
              AND source_reference.file_version_id = re.source_file_version_id
              AND source_reference.observation_id = re.source_observation_id
             LEFT JOIN call_sites AS source_call
               ON re.source_category = 'call_site'
              AND source_call.file_version_id = re.source_file_version_id
              AND source_call.observation_id = re.source_observation_id
             LEFT JOIN symbols AS source_symbol
               ON re.source_category = 'symbol'
              AND source_symbol.file_version_id = re.source_file_version_id
              AND source_symbol.observation_id = re.source_observation_id
             WHERE re.generation_id = ?1 AND re.target_symbol_id = ?2
               AND re.relationship IN ('references', 'calls')
               AND (?3 = 1 OR re.resolution <> 'candidate')
               AND (?4 IS NULL OR source_file.relative_path > ?4
                    OR (source_file.relative_path = ?4 AND re.source_start_byte > ?5)
                    OR (source_file.relative_path = ?4 AND re.source_start_byte = ?5
                        AND re.edge_id > ?6))
             ORDER BY source_file.relative_path, re.source_start_byte, re.edge_id
             LIMIT ?7",
        )?;
        let rows = statement.query_map(
            params![
                self.generation_id.as_str(),
                symbol_id,
                if include_candidates { 1_i64 } else { 0_i64 },
                after_path,
                after_start,
                after_edge,
                to_sql_limit(limit)?,
            ],
            |row| {
                Ok(StoredReferenceRecord {
                    edge_id: row.get(0)?,
                    source_observation_id: row.get(1)?,
                    relative_path: row.get(2)?,
                    spelling: row.get(3)?,
                    start_byte: row_u64(row, 4)?,
                    end_byte: row_u64(row, 5)?,
                    resolution: row.get(6)?,
                    candidate_count: row_u64(row, 7)?,
                    rule_version: row.get(8)?,
                    limitations_json: row.get(9)?,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn retrieval_adjacent_relations(
        &self,
        symbol_id: &str,
        direction: StoredGraphDirection,
        relationship: Option<&str>,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<StoredRelationRecord>, StorageError> {
        validate_retrieval_text(symbol_id, 512, "symbol ID")?;
        validate_retrieval_limit(limit, 2_001)?;
        if relationship.is_some_and(|value| !matches!(value, "calls" | "references")) {
            return Err(StorageError::InvalidInput(
                "unsupported relationship filter".to_owned(),
            ));
        }
        let predicate = match direction {
            StoredGraphDirection::Incoming => "re.target_symbol_id = ?2",
            StoredGraphDirection::Outgoing => "re.source_symbol_id = ?2",
        };
        self.retrieval_relations(
            predicate,
            Some(symbol_id),
            relationship,
            include_candidates,
            limit,
        )
    }

    pub fn retrieval_incoming_relations_to_path(
        &self,
        relative_path: &str,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<StoredRelationRecord>, StorageError> {
        validate_retrieval_text(relative_path, 4_096, "relative path")?;
        validate_retrieval_limit(limit, 2_001)?;
        self.retrieval_relations(
            "target_file.relative_path = ?2",
            Some(relative_path),
            None,
            include_candidates,
            limit,
        )
    }

    fn retrieval_relations(
        &self,
        predicate: &'static str,
        subject: Option<&str>,
        relationship: Option<&str>,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<StoredRelationRecord>, StorageError> {
        let sql = format!(
            "SELECT re.edge_id, re.relationship, re.source_symbol_id,
                    re.source_observation_id, source_file.relative_path,
                    re.source_start_byte, re.target_symbol_id,
                    target_file.relative_path, re.resolution, re.candidate_count,
                    re.rule_version, re.reason, re.limitations_json
             FROM resolved_edges AS re
             JOIN file_versions AS source_version
               ON source_version.id = re.source_file_version_id
             JOIN files AS source_file ON source_file.id = source_version.file_id
             LEFT JOIN file_versions AS target_version
               ON target_version.id = re.target_file_version_id
             LEFT JOIN files AS target_file ON target_file.id = target_version.file_id
             WHERE re.generation_id = ?1 AND {predicate}
               AND (?3 IS NULL OR re.relationship = ?3)
               AND (?4 = 1 OR re.resolution <> 'candidate')
             ORDER BY source_file.relative_path, re.source_start_byte, re.edge_id
             LIMIT ?5"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(
            params![
                self.generation_id.as_str(),
                subject,
                relationship,
                if include_candidates { 1_i64 } else { 0_i64 },
                to_sql_limit(limit)?,
            ],
            |row| {
                Ok(StoredRelationRecord {
                    id: row.get(0)?,
                    relationship: row.get(1)?,
                    source_symbol_id: row.get(2)?,
                    source_observation_id: row.get(3)?,
                    source_path: row.get(4)?,
                    source_start_byte: row_u64(row, 5)?,
                    target_symbol_id: row.get(6)?,
                    target_path: row.get(7)?,
                    resolution: row.get(8)?,
                    candidate_count: row_u64(row, 9)?,
                    rule_version: row.get(10)?,
                    reason: row.get(11)?,
                    limitations_json: row.get(12)?,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn graph_edges(
        &self,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<StoredGraphEdge>, StorageError> {
        validate_graph_read_limit(limit)?;
        read_graph_edges(
            &self.connection,
            self.generation_id.as_str(),
            None,
            None,
            include_candidates,
            limit,
        )
    }

    pub fn graph_facts(
        &self,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<StoredGraphFact>, StorageError> {
        validate_graph_read_limit(limit)?;
        let limit = i64::try_from(limit)
            .map_err(|_| StorageError::InvalidInput("graph limit overflow".to_owned()))?;
        let mut statement = self.connection.prepare(
            "SELECT re.edge_id, re.relationship, re.source_symbol_id,
                    re.source_observation_id, re.target_symbol_id, re.resolution,
                    source_file.relative_path, re.source_category,
                    COALESCE(
                        source_symbol.spelling, source_scope.spelling, source_import.spelling,
                        source_reference.spelling, source_call.spelling, ''
                    ),
                    re.source_start_byte, target_file.relative_path, target_symbol.spelling,
                    re.candidate_count, re.rule_version
             FROM resolved_edges AS re
             JOIN file_versions AS source_version ON source_version.id = re.source_file_version_id
             JOIN files AS source_file ON source_file.id = source_version.file_id
             LEFT JOIN symbols AS source_symbol
               ON re.source_category = 'symbol'
              AND source_symbol.file_version_id = re.source_file_version_id
              AND source_symbol.observation_id = re.source_observation_id
             LEFT JOIN scopes AS source_scope
               ON re.source_category = 'scope'
              AND source_scope.file_version_id = re.source_file_version_id
              AND source_scope.observation_id = re.source_observation_id
             LEFT JOIN imports AS source_import
               ON re.source_category = 'import'
              AND source_import.file_version_id = re.source_file_version_id
              AND source_import.observation_id = re.source_observation_id
             LEFT JOIN \"references\" AS source_reference
               ON re.source_category = 'reference'
              AND source_reference.file_version_id = re.source_file_version_id
              AND source_reference.observation_id = re.source_observation_id
             LEFT JOIN call_sites AS source_call
               ON re.source_category = 'call_site'
              AND source_call.file_version_id = re.source_file_version_id
              AND source_call.observation_id = re.source_observation_id
             LEFT JOIN file_versions AS target_version ON target_version.id = re.target_file_version_id
             LEFT JOIN files AS target_file ON target_file.id = target_version.file_id
             LEFT JOIN symbols AS target_symbol
               ON target_symbol.file_version_id = re.target_file_version_id
              AND target_symbol.observation_id = re.target_symbol_id
             WHERE re.generation_id = ?1
               AND (?2 = 1 OR re.resolution <> 'candidate')
             ORDER BY source_file.relative_path, re.source_start_byte, re.edge_id
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![
                self.generation_id.as_str(),
                if include_candidates { 1_i64 } else { 0_i64 },
                limit,
            ],
            |row| {
                Ok((
                    StoredGraphEdge {
                        id: row.get(0)?,
                        relationship: row.get(1)?,
                        source_symbol_id: row.get(2)?,
                        source_observation_id: row.get(3)?,
                        target_symbol_id: row.get(4)?,
                        resolution: row.get(5)?,
                    },
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, String>(13)?,
                ))
            },
        )?;
        let mut facts = Vec::new();
        for row in rows {
            let (
                edge,
                source_path,
                source_category,
                source_spelling,
                source_start_byte,
                target_path,
                target_name,
                candidate_count,
                rule_version,
            ) = row?;
            facts.push(StoredGraphFact {
                edge,
                source_path,
                source_category,
                source_spelling,
                source_start_byte: u64::try_from(source_start_byte).map_err(|_| {
                    StorageError::InvalidInput("negative stored edge source".to_owned())
                })?,
                target_path,
                target_name,
                candidate_count: u64::try_from(candidate_count).map_err(|_| {
                    StorageError::InvalidInput("negative stored candidate count".to_owned())
                })?,
                rule_version,
            });
        }
        Ok(facts)
    }

    pub fn adjacent_graph_edges(
        &self,
        symbol_id: &str,
        direction: StoredGraphDirection,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<StoredGraphEdge>, StorageError> {
        validate_graph_symbol_and_limit(symbol_id, limit)?;
        read_graph_edges(
            &self.connection,
            self.generation_id.as_str(),
            Some(symbol_id),
            Some(direction),
            include_candidates,
            limit,
        )
    }

    pub fn call_site_edges(
        &self,
        symbol_id: &str,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<StoredGraphEdge>, StorageError> {
        validate_graph_symbol_and_limit(symbol_id, limit)?;
        let limit = i64::try_from(limit)
            .map_err(|_| StorageError::InvalidInput("graph limit overflow".to_owned()))?;
        let mut statement = self.connection.prepare(
            "SELECT edge_id, relationship, source_symbol_id, source_observation_id,
                    target_symbol_id, resolution
             FROM resolved_edges
             WHERE generation_id = ?1 AND relationship = 'calls'
               AND target_symbol_id = ?2
               AND (?3 = 1 OR resolution <> 'candidate')
             ORDER BY edge_id LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![
                self.generation_id.as_str(),
                symbol_id,
                if include_candidates { 1_i64 } else { 0_i64 },
                limit,
            ],
            stored_graph_edge_from_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }
}

fn validate_retrieval_text(value: &str, maximum: usize, field: &str) -> Result<(), StorageError> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(StorageError::InvalidInput(format!(
            "{field} must contain 1..={maximum} bytes and no NUL"
        )));
    }
    Ok(())
}

fn validate_retrieval_limit(limit: usize, maximum: usize) -> Result<(), StorageError> {
    if !(1..=maximum).contains(&limit) {
        return Err(StorageError::InvalidInput(format!(
            "retrieval limit must be in 1..={maximum}"
        )));
    }
    Ok(())
}

fn qualified_name_leaf(query: &str) -> Option<&str> {
    let rust = query.rfind("::").map(|index| index.saturating_add(2));
    let dotted = query.rfind('.').map(|index| index.saturating_add(1));
    let start = rust.into_iter().chain(dotted).max()?;
    query.get(start..).filter(|leaf| !leaf.is_empty())
}

fn validate_search_plan(plan: &StoredSearchPlan) -> Result<(), StorageError> {
    validate_retrieval_text(&plan.query, MAX_SEARCH_QUERY_BYTES, "search query")?;
    validate_retrieval_text(
        &plan.folded_query,
        MAX_SEARCH_QUERY_BYTES.saturating_mul(4),
        "folded search query",
    )?;
    validate_retrieval_text(&plan.fts_expression, 4_096, "FTS expression")?;
    validate_retrieval_limit(plan.fetch_limit, 201)?;
    if !plan
        .fts_expression
        .chars()
        .all(|character| character.is_alphanumeric() || matches!(character, '"' | ' ' | '_'))
    {
        return Err(StorageError::InvalidInput(
            "FTS expression is not a literal token expression".to_owned(),
        ));
    }
    Ok(())
}

fn to_sql_limit(limit: usize) -> Result<i64, StorageError> {
    i64::try_from(limit).map_err(|_| StorageError::InvalidInput("limit overflow".to_owned()))
}

fn nonnegative_u64(value: i64, field: &str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::InvalidInput(format!("negative stored {field}")))
}

fn row_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(index)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn escape_sql_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn stored_symbol_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSymbolRecord> {
    Ok(StoredSymbolRecord {
        id: row.get(0)?,
        relative_path: row.get(1)?,
        language: row.get(2)?,
        kind: row.get(3)?,
        name: row.get(4)?,
        container: row.get(5)?,
        signature: row.get(6)?,
        start_byte: row_u64(row, 7)?,
        end_byte: row_u64(row, 8)?,
        syntax_start_byte: row_u64(row, 9)?,
        syntax_end_byte: row_u64(row, 10)?,
        content_hash: row.get(11)?,
        parse_status: row.get(12)?,
        attributes_json: row.get(13)?,
        limitations_json: row.get(14)?,
    })
}

fn stored_indexed_file_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredIndexedFile> {
    Ok(StoredIndexedFile {
        relative_path: row.get(0)?,
        content_hash: row.get(1)?,
        language: row.get(2)?,
        parse_status: row.get(3)?,
        coverage_json: row.get(4)?,
        byte_length: row_u64(row, 5)?,
        symbol_count: row_u64(row, 6)?,
    })
}

fn stored_search_order(
    left: &StoredSearchSymbol,
    right: &StoredSearchSymbol,
) -> std::cmp::Ordering {
    left.tier
        .cmp(&right.tier)
        .then_with(|| left.fts_score.total_cmp(&right.fts_score))
        .then_with(|| left.symbol.relative_path.cmp(&right.symbol.relative_path))
        .then_with(|| left.symbol.name.cmp(&right.symbol.name))
        .then_with(|| left.symbol.start_byte.cmp(&right.symbol.start_byte))
        .then_with(|| left.symbol.id.cmp(&right.symbol.id))
}

fn stored_search_after(candidate: &StoredSearchSymbol, after: &StoredSearchAfter) -> bool {
    candidate
        .tier
        .cmp(&after.tier)
        .then_with(|| candidate.fts_score.total_cmp(&after.fts_score))
        .then_with(|| candidate.symbol.relative_path.cmp(&after.relative_path))
        .then_with(|| candidate.symbol.name.cmp(&after.name))
        .then_with(|| candidate.symbol.start_byte.cmp(&after.start_byte))
        .then_with(|| candidate.symbol.id.cmp(&after.symbol_id))
        .is_gt()
}

fn validate_graph_read_limit(limit: usize) -> Result<(), StorageError> {
    if !(1..=10_000).contains(&limit) {
        return Err(StorageError::InvalidInput(
            "graph limit must be in 1..=10000".to_owned(),
        ));
    }
    Ok(())
}

fn validate_graph_symbol_and_limit(symbol_id: &str, limit: usize) -> Result<(), StorageError> {
    validate_graph_read_limit(limit)?;
    if symbol_id.is_empty() || symbol_id.len() > 256 {
        return Err(StorageError::InvalidInput(
            "graph symbol ID must be in 1..=256 bytes".to_owned(),
        ));
    }
    Ok(())
}

fn read_graph_edges(
    connection: &Connection,
    generation_id: &str,
    symbol_id: Option<&str>,
    direction: Option<StoredGraphDirection>,
    include_candidates: bool,
    limit: usize,
) -> Result<Vec<StoredGraphEdge>, StorageError> {
    let limit = i64::try_from(limit)
        .map_err(|_| StorageError::InvalidInput("graph limit overflow".to_owned()))?;
    let (predicate, symbol) = match direction {
        Some(StoredGraphDirection::Incoming) => ("target_symbol_id = ?2", symbol_id),
        Some(StoredGraphDirection::Outgoing) => ("source_symbol_id = ?2", symbol_id),
        None => ("?2 IS NULL", None),
    };
    let sql = format!(
        "SELECT edge_id, relationship, source_symbol_id, source_observation_id,
                target_symbol_id, resolution
         FROM resolved_edges
         WHERE generation_id = ?1 AND {predicate}
           AND (?3 = 1 OR resolution <> 'candidate')
         ORDER BY edge_id LIMIT ?4"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            generation_id,
            symbol,
            if include_candidates { 1_i64 } else { 0_i64 },
            limit,
        ],
        stored_graph_edge_from_row,
    )?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

fn stored_graph_edge_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredGraphEdge> {
    Ok(StoredGraphEdge {
        id: row.get(0)?,
        relationship: row.get(1)?,
        source_symbol_id: row.get(2)?,
        source_observation_id: row.get(3)?,
        target_symbol_id: row.get(4)?,
        resolution: row.get(5)?,
    })
}

impl Drop for ReadSnapshot {
    fn drop(&mut self) {
        let _ = self.connection.execute_batch("ROLLBACK");
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestLayout {
        base: PathBuf,
        root: PathBuf,
        paths: StoragePaths,
        repository_id: RepositoryId,
    }

    impl TestLayout {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock follows Unix epoch")
                .as_nanos();
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let base = std::env::temp_dir().join(format!(
                "codeatlas-storage-{label}-{}-{nonce}-{sequence}",
                std::process::id()
            ));
            let root = base.join("repository");
            let data_base = base.join("application-data");
            fs::create_dir_all(&root).expect("create source root");
            fs::create_dir_all(&data_base).expect("create data base");
            let repository_id = RepositoryId::new(format!("repo-{label}-{sequence}"))
                .expect("valid test repository ID");
            let paths = StoragePaths::under(&data_base, &repository_id, &root)
                .expect("valid isolated storage paths");
            Self {
                base,
                root,
                paths,
                repository_id,
            }
        }

        fn open(&self) -> Storage {
            Storage::open(self.repository_id.clone(), self.paths.clone())
                .expect("open test storage")
        }
    }

    impl Drop for TestLayout {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn staged_file(id: &str, fact_name: &str, hash_character: char) -> StagedFile {
        StagedFile {
            relative_path: "src/lib.rs".to_owned(),
            content_hash: std::iter::repeat_n(hash_character, 64).collect(),
            extractor_hash: "extractor-v1".to_owned(),
            language: "rust".to_owned(),
            grammar_hash: "grammar-v1".to_owned(),
            query_hash: "query-v1".to_owned(),
            parse_status: "complete".to_owned(),
            coverage_json: "{}".to_owned(),
            byte_length: 64,
            facts: vec![StoredFact {
                id: format!("fact-{id}"),
                kind: "function".to_owned(),
                name: fact_name.to_owned(),
                byte_range: ByteRange::new(0, 10).expect("valid byte range"),
            }],
            observations: vec![StoredObservation {
                category: ObservationCategory::Symbol,
                id: format!("symbol-{id}"),
                kind: "function".to_owned(),
                spelling: fact_name.to_owned(),
                byte_range: ByteRange::new(0, 10).expect("valid byte range"),
                syntax_range: ByteRange::new(0, 32).expect("valid syntax range"),
                scope_id: None,
                container: None,
                signature: Some(format!("fn {fact_name}()")),
                receiver: None,
                alias: None,
                target_id: None,
                resolution: "syntax_observation".to_owned(),
                attributes_json: "[]".to_owned(),
                limitations_json: "[]".to_owned(),
            }],
            diagnostics: Vec::new(),
            search_documents: vec![SearchDocument {
                name: fact_name.to_owned(),
                content: format!("pub fn {fact_name} implementation"),
            }],
        }
    }

    fn start_generation(
        storage: &Storage,
        id: &str,
        fact_name: &str,
        hash_character: char,
    ) -> JobRecord {
        start_generation_with_file(storage, id, staged_file(id, fact_name, hash_character))
    }

    fn start_generation_with_file(storage: &Storage, id: &str, file: StagedFile) -> JobRecord {
        start_generation_with_config(storage, id, file, "config-v1")
    }

    fn start_generation_with_config(
        storage: &Storage,
        id: &str,
        file: StagedFile,
        config: &str,
    ) -> JobRecord {
        let job = storage
            .create_job(NewJob {
                mode: "full".to_owned(),
                request_key: None,
                owner_instance_id: "test-owner".to_owned(),
            })
            .expect("create test job")
            .job;
        storage
            .update_job(
                job.id.clone(),
                JobState::Scanning,
                JobProgress::default(),
                None,
            )
            .expect("start scanning");
        storage
            .update_job(
                job.id.clone(),
                JobState::Parsing,
                JobProgress::default(),
                None,
            )
            .expect("start parsing");
        let generation_id = GenerationId::new(id).expect("valid generation ID");
        storage
            .begin_generation(GenerationPlan {
                generation_id: generation_id.clone(),
                job_id: job.id.clone(),
                config_hash: config.to_owned(),
                extractor_set_hash: "extractor-set-v1".to_owned(),
                reuse_parent: false,
                deleted_paths: Vec::new(),
            })
            .expect("begin generation");
        storage
            .stage_files(generation_id, vec![file])
            .expect("stage generation file");
        job
    }

    fn finish_generation(storage: &Storage, id: &str, job: &JobRecord, scan_complete: bool) {
        let generation_id = GenerationId::new(id).expect("valid generation ID");
        storage
            .replace_resolution_graph(StoredResolutionGraph {
                generation_id: generation_id.clone(),
                resolver_version: "test-resolver-v1".to_owned(),
                edges: Vec::new(),
                external_nodes: Vec::new(),
                diagnostics: Vec::new(),
                unresolved_count: 0,
            })
            .expect("record empty resolved graph");
        storage
            .complete_generation(GenerationCompletion {
                generation_id,
                scan_complete,
                coverage_status: "syntax_and_name_resolution".to_owned(),
                warning_count: 0,
                failure_summary: None,
            })
            .expect("finish generation staging");
        storage
            .update_job(
                job.id.clone(),
                JobState::Resolving,
                JobProgress::default(),
                None,
            )
            .expect("record syntax-only resolution phase");
        storage
            .update_job(
                job.id.clone(),
                JobState::Committing,
                JobProgress::default(),
                None,
            )
            .expect("record committing phase");
    }

    fn stage_and_activate(storage: &Storage, id: &str, fact_name: &str, hash_character: char) {
        let job = start_generation(storage, id, fact_name, hash_character);
        finish_generation(storage, id, &job, true);
        storage
            .activate_generation(GenerationId::new(id).expect("valid generation ID"))
            .expect("activate complete generation");
    }

    #[test]
    fn complete_analysis_identity_keeps_versions_immutable() {
        let layout = TestLayout::new("analysis-identity");
        let storage = layout.open();
        stage_and_activate(&storage, "initial", "original", 'a');
        let original = storage.read_snapshot().expect("pinned original");
        let mut file = staged_file("changed", "changed", 'a');
        for index in 0..4 {
            match index {
                0 => file.grammar_hash = "grammar-v2".into(),
                1 => file.query_hash = "query-v2".into(),
                2 => file.extractor_hash = "extractor-v2".into(),
                _ => {}
            }
            let id = format!("analysis-{index}");
            let job = start_generation_with_config(
                &storage,
                &id,
                file.clone(),
                if index == 3 { "config-v2" } else { "config-v1" },
            );
            finish_generation(&storage, &id, &job, true);
            storage
                .activate_generation(GenerationId::new(&id).expect("id"))
                .expect("activate");
        }
        let mut changed_payload = file;
        changed_payload.facts[0].name = "must_not_append".into();
        changed_payload.facts[0].id = "new-fact-id".into();
        let job = start_generation_with_config(&storage, "reuse", changed_payload, "config-v2");
        finish_generation(&storage, "reuse", &job, true);
        storage
            .activate_generation(GenerationId::new("reuse").expect("id"))
            .expect("activate reuse");
        assert_eq!(
            original.facts().expect("original unchanged")[0].name,
            "original"
        );
        assert_eq!(
            storage
                .read_snapshot()
                .expect("new snapshot")
                .facts()
                .expect("immutable reused facts")[0]
                .name,
            "changed"
        );
        let connection =
            open_reader_connection(layout.paths.database_path()).expect("read versions");
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM file_versions", [], |row| row
                    .get::<_, i64>(0))
                .expect("version count"),
            5
        );
        assert_eq!(
            storage.active_files().expect("active fingerprint")[0].config_hash,
            "config-v2"
        );
    }

    #[test]
    fn sql_keysets_page_across_all_ranking_tiers_without_fts_duplicates() {
        let layout = TestLayout::new("keyset-tiers");
        let storage = layout.open();
        let mut file = staged_file("ranking", "work", 'a');
        let template = file.observations[0].clone();
        file.observations.clear();
        file.search_documents.clear();
        file.byte_length = 10_000;
        for (index, name) in ["work", "WORK", "workAlpha", "WorkBeta", "Other"]
            .iter()
            .cycle()
            .take(100)
            .enumerate()
        {
            let mut symbol = template.clone();
            symbol.id = format!("symbol-{index}");
            symbol.spelling = (*name).into();
            symbol.container = Some("Class".into());
            symbol.byte_range =
                ByteRange::new(index as u64 * 32, index as u64 * 32 + 6).expect("range");
            symbol.syntax_range =
                ByteRange::new(index as u64 * 32, index as u64 * 32 + 32).expect("syntax");
            file.observations.push(symbol);
            file.search_documents.push(SearchDocument {
                name: (*name).into(),
                content: format!("work evidence {index}"),
            });
        }
        let job = start_generation_with_file(&storage, "ranking", file);
        finish_generation(&storage, "ranking", &job, true);
        storage
            .activate_generation(GenerationId::new("ranking").expect("id"))
            .expect("activate");
        let snapshot = storage.read_snapshot().expect("snapshot");
        for (query, fts, expected) in [
            ("work", "\"work\"", 100),
            ("Class.work", "\"Class\" AND \"work\"", 20),
        ] {
            let mut plan = StoredSearchPlan {
                query: query.into(),
                folded_query: query.to_lowercase(),
                escaped_folded_prefix: format!("{}%", query.to_lowercase()),
                fts_expression: fts.into(),
                language: None,
                kind: None,
                path_prefix: None,
                after: None,
                fetch_limit: 7,
            };
            let mut seen = HashSet::new();
            let mut tiers = BTreeSet::new();
            loop {
                let rows = snapshot
                    .retrieval_search_symbols(&plan)
                    .expect("keyset page");
                if rows.is_empty() {
                    break;
                }
                for row in &rows {
                    assert!(
                        seen.insert(row.symbol.id.clone()),
                        "no duplicate across tiers/pages"
                    );
                    tiers.insert(row.tier);
                }
                let last = rows.last().expect("nonempty page");
                plan.after = Some(StoredSearchAfter {
                    tier: last.tier,
                    fts_score: last.fts_score,
                    relative_path: last.symbol.relative_path.clone(),
                    name: last.symbol.name.clone(),
                    start_byte: last.symbol.start_byte,
                    symbol_id: last.symbol.id.clone(),
                });
            }
            assert_eq!(seen.len(), expected);
            assert_eq!(
                tiers,
                if query == "work" {
                    BTreeSet::from([1, 2, 3, 4, 5])
                } else {
                    BTreeSet::from([0])
                }
            );
        }
    }

    #[test]
    fn generation_retention_keeps_active_previous_and_pinned_readers_not_history() {
        let layout = TestLayout::new("bounded-history");
        let storage = layout.open();
        storage
            .put_memory("sentinel".into(), "keep memory".into())
            .expect("memory");
        stage_and_activate(&storage, "first", "first", 'a');
        let pinned = storage.read_snapshot().expect("old snapshot");
        for index in 0..100 {
            let id = format!("revision-{index}");
            stage_and_activate(&storage, &id, &id, if index % 2 == 0 { 'b' } else { 'c' });
            storage.gc_abandoned().expect("bounded GC");
        }
        assert_eq!(
            pinned.facts().expect("pinned WAL snapshot")[0].name,
            "first"
        );
        let connection = open_reader_connection(layout.paths.database_path()).expect("reader");
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM generations", [], |row| row
                    .get::<_, i64>(0))
                .expect("generations"),
            2
        );
        assert!(
            connection
                .query_row("SELECT count(*) FROM file_versions", [], |row| row
                    .get::<_, i64>(0))
                .expect("versions")
                <= 2
        );
        assert!(
            connection
                .prepare("PRAGMA foreign_key_check")
                .expect("FK check")
                .query([])
                .expect("FK rows")
                .next()
                .expect("FK result")
                .is_none()
        );
        assert_eq!(
            storage
                .read_memory("sentinel")
                .expect("memory survives")
                .as_deref(),
            Some("keep memory")
        );
    }

    #[test]
    fn schema_seven_preserves_legacy_version_ids_and_invalidates_cache() {
        let mut connection = Connection::open_in_memory().expect("legacy DB");
        configure_connection(&connection, false).expect("configure");
        schema::migrate_to(&mut connection, "repo-legacy", 6).expect("v6");
        connection.execute_batch("INSERT INTO files(id, relative_path) VALUES(42, 'a.rs');
            INSERT INTO file_versions(id, file_id, content_hash, extractor_hash, byte_length, source_encoding) VALUES(99,42,'hash','extractor-old',10,'utf-8');
            INSERT INTO facts(id,file_version_id,kind,name,start_byte,end_byte) VALUES('fact',99,'function','old',0,3);
            INSERT INTO memories(id,body,created_at) VALUES('sentinel','private note',1);").expect("legacy rows");
        schema::migrate(&mut connection, "repo-legacy").expect("v7 migration");
        let fingerprint: (i64, String, String) = connection
            .query_row(
                "SELECT id, extractor_hash, config_hash FROM file_versions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("legacy identity");
        assert_eq!(fingerprint, (99, "extractor-old".into(), "".into()));
        assert_eq!(
            connection
                .query_row("SELECT file_version_id FROM facts", [], |row| row
                    .get::<_, i64>(0))
                .expect("fact survives"),
            99
        );
        assert_eq!(
            connection
                .query_row("SELECT body FROM memories", [], |row| row
                    .get::<_, String>(0))
                .expect("note survives"),
            "private note"
        );
        assert!(
            connection
                .prepare("PRAGMA foreign_key_check")
                .expect("FK check")
                .query([])
                .expect("FK rows")
                .next()
                .expect("FK result")
                .is_none()
        );
    }

    #[test]
    fn schema_seven_failure_rolls_back_column_rename_and_fingerprints() {
        let mut connection = Connection::open_in_memory().expect("legacy DB");
        configure_connection(&connection, false).expect("configure");
        schema::migrate_to(&mut connection, "repo-rollback", 6).expect("v6");
        connection.execute_batch("INSERT INTO files(id,relative_path) VALUES(1,'a.rs');
            INSERT INTO file_versions(file_id,content_hash,extractor_hash,byte_length,source_encoding) VALUES(1,'hash','old',1,'utf-8');
            CREATE TRIGGER refuse_version_update BEFORE UPDATE ON file_versions BEGIN SELECT RAISE(FAIL,'injected migration failure'); END;").expect("failure fixture");
        assert!(schema::migrate(&mut connection, "repo-rollback").is_err());
        assert_eq!(schema::schema_version(&connection).expect("version"), 6);
        assert_eq!(
            connection
                .query_row("SELECT extractor_hash FROM file_versions", [], |row| row
                    .get::<_, String>(
                    0
                ))
                .expect("old column intact"),
            "old"
        );
        assert!(
            connection
                .prepare("SELECT analysis_key FROM file_versions")
                .is_err()
        );
    }

    #[test]
    fn generated_activations_never_mix_generation_join_rows() {
        let layout = TestLayout::new("generation-property");
        let storage = layout.open();
        for index in 0_u8..16 {
            let generation = format!("generation-{index}");
            let fact_name = format!("fact_name_{index}");
            let hash_character = char::from(b"0123456789abcdef"[usize::from(index)]);
            stage_and_activate(&storage, &generation, &fact_name, hash_character);

            let snapshot = storage.read_snapshot().expect("read active snapshot");
            assert_eq!(snapshot.generation_id().as_str(), generation);
            let facts = snapshot.facts().expect("read generation-scoped facts");
            assert_eq!(facts.len(), 1);
            assert_eq!(facts[0].id, format!("fact-{generation}"));
            assert_eq!(facts[0].name, fact_name);
            let files = storage.active_files().expect("read active files");
            assert_eq!(files.len(), 1);
            assert_eq!(
                files[0].content_hash,
                std::iter::repeat_n(hash_character, 64).collect::<String>()
            );
        }
    }

    #[test]
    fn edge_free_resolution_diagnostics_still_validate_generation_membership() {
        let layout = TestLayout::new("diagnostic-membership");
        let storage = layout.open();
        let generation = GenerationId::new("generation-diagnostic").expect("generation ID");
        let _job = start_generation(&storage, generation.as_str(), "diagnosed", 'd');

        storage
            .replace_resolution_graph(StoredResolutionGraph {
                generation_id: generation,
                resolver_version: "test-resolver-v1".to_owned(),
                edges: Vec::new(),
                external_nodes: Vec::new(),
                diagnostics: vec![StoredResolutionDiagnostic {
                    relative_path: "src/lib.rs".to_owned(),
                    observation_id: Some("symbol-generation-diagnostic".to_owned()),
                    code: "test_diagnostic".to_owned(),
                    message: "edge-free graphs still retain diagnostics".to_owned(),
                }],
                unresolved_count: 1,
            })
            .expect("persist edge-free file-scoped diagnostic");
    }

    #[test]
    fn jobs_deduplicate_request_keys_and_persist_cancellation() {
        let layout = TestLayout::new("jobs");
        let storage = layout.open();
        let request = NewJob {
            mode: "incremental".to_owned(),
            request_key: Some("caller-operation-1".to_owned()),
            owner_instance_id: "test-owner".to_owned(),
        };
        let created = storage.create_job(request.clone()).expect("create job");
        assert!(!created.deduplicated);
        let repeated = storage.create_job(request).expect("deduplicate job");
        assert!(repeated.deduplicated);
        assert_eq!(created.job.id, repeated.job.id);
        assert_eq!(
            storage
                .request_cancel(created.job.id.clone())
                .expect("request cancellation"),
            CancelJobResult::Requested
        );
        assert_eq!(
            storage
                .request_cancel(created.job.id.clone())
                .expect("repeat cancellation"),
            CancelJobResult::AlreadyRequested
        );
        storage
            .abandon_generation(
                None,
                created.job.id.clone(),
                JobState::Cancelled,
                JobProgress::default(),
                "cancelled by caller".to_owned(),
            )
            .expect("record cancelled terminal state");
        assert_eq!(
            storage
                .request_cancel(created.job.id)
                .expect("request cancellation after completion"),
            CancelJobResult::AlreadyCompleted
        );
    }

    #[test]
    fn activated_deletion_removes_facts_and_fts_from_the_selected_generation() {
        let layout = TestLayout::new("deletion-membership");
        let storage = layout.open();
        stage_and_activate(&storage, "generation-with-file", "alpha", 'a');

        let job = storage
            .create_job(NewJob {
                mode: "incremental".to_owned(),
                request_key: None,
                owner_instance_id: "test-owner".to_owned(),
            })
            .expect("create deletion job")
            .job;
        storage
            .update_job(
                job.id.clone(),
                JobState::Scanning,
                JobProgress::default(),
                None,
            )
            .expect("start deletion scan");
        storage
            .update_job(
                job.id.clone(),
                JobState::Parsing,
                JobProgress::default(),
                None,
            )
            .expect("start deletion parse");
        let generation_id = GenerationId::new("generation-empty").expect("generation ID");
        storage
            .begin_generation(GenerationPlan {
                generation_id: generation_id.clone(),
                job_id: job.id.clone(),
                config_hash: "config-v1".to_owned(),
                extractor_set_hash: "extractor-set-v1".to_owned(),
                reuse_parent: false,
                deleted_paths: Vec::new(),
            })
            .expect("begin empty generation");
        storage
            .replace_resolution_graph(StoredResolutionGraph {
                generation_id: generation_id.clone(),
                resolver_version: "test-resolver-v1".to_owned(),
                edges: Vec::new(),
                external_nodes: Vec::new(),
                diagnostics: Vec::new(),
                unresolved_count: 0,
            })
            .expect("resolve empty generation");
        storage
            .complete_generation(GenerationCompletion {
                generation_id: generation_id.clone(),
                scan_complete: true,
                coverage_status: "syntax_and_name_resolution".to_owned(),
                warning_count: 0,
                failure_summary: None,
            })
            .expect("complete empty generation");
        storage
            .update_job(
                job.id.clone(),
                JobState::Resolving,
                JobProgress::default(),
                None,
            )
            .expect("resolve empty generation");
        let completed_job_id = job.id.clone();
        storage
            .update_job(job.id, JobState::Committing, JobProgress::default(), None)
            .expect("commit empty generation");
        storage
            .activate_generation(generation_id)
            .expect("activate empty generation");
        assert_eq!(
            storage
                .request_cancel(completed_job_id)
                .expect("cancel after atomic activation"),
            CancelJobResult::AlreadyCompleted
        );

        let snapshot = storage.read_snapshot().expect("read empty generation");
        assert!(snapshot.facts().expect("read facts").is_empty());
        assert!(snapshot.search("alpha", 10).expect("search FTS").is_empty());
    }

    #[test]
    fn graph_edges_are_generation_scoped_and_candidates_require_opt_in() {
        let layout = TestLayout::new("resolved-graph");
        let storage = layout.open();
        let job = storage
            .create_job(NewJob {
                mode: "full".to_owned(),
                request_key: None,
                owner_instance_id: "test-owner".to_owned(),
            })
            .expect("create graph job")
            .job;
        storage
            .update_job(
                job.id.clone(),
                JobState::Scanning,
                JobProgress::default(),
                None,
            )
            .expect("start graph scan");
        storage
            .update_job(
                job.id.clone(),
                JobState::Parsing,
                JobProgress::default(),
                None,
            )
            .expect("start graph parse");
        let generation = GenerationId::new("generation-graph").expect("generation ID");
        storage
            .begin_generation(GenerationPlan {
                generation_id: generation.clone(),
                job_id: job.id.clone(),
                config_hash: "config-v1".to_owned(),
                extractor_set_hash: "extractor-set-v1".to_owned(),
                reuse_parent: false,
                deleted_paths: Vec::new(),
            })
            .expect("begin graph generation");
        let mut provider = staged_file("provider", "provide", 'a');
        provider.relative_path = "src/provider.rs".to_owned();
        let mut consumer = staged_file("caller", "caller", 'b');
        consumer.relative_path = "src/caller.rs".to_owned();
        consumer.observations.extend([
            StoredObservation {
                category: ObservationCategory::Reference,
                id: "reference-provide".to_owned(),
                kind: "identifier".to_owned(),
                spelling: "provide".to_owned(),
                byte_range: ByteRange::new(20, 27).expect("range"),
                syntax_range: ByteRange::new(20, 27).expect("range"),
                scope_id: None,
                container: Some("caller".to_owned()),
                signature: None,
                receiver: None,
                alias: None,
                target_id: None,
                resolution: "unresolved".to_owned(),
                attributes_json: "[]".to_owned(),
                limitations_json: "[]".to_owned(),
            },
            StoredObservation {
                category: ObservationCategory::CallSite,
                id: "call-provide-candidate".to_owned(),
                kind: "function".to_owned(),
                spelling: "provide".to_owned(),
                byte_range: ByteRange::new(30, 37).expect("range"),
                syntax_range: ByteRange::new(30, 39).expect("range"),
                scope_id: None,
                container: Some("caller".to_owned()),
                signature: None,
                receiver: None,
                alias: None,
                target_id: None,
                resolution: "unresolved".to_owned(),
                attributes_json: "[]".to_owned(),
                limitations_json: "[]".to_owned(),
            },
            StoredObservation {
                category: ObservationCategory::CallSite,
                id: "call-dynamic".to_owned(),
                kind: "method".to_owned(),
                spelling: "value[key]".to_owned(),
                byte_range: ByteRange::new(40, 50).expect("range"),
                syntax_range: ByteRange::new(40, 52).expect("range"),
                scope_id: None,
                container: Some("caller".to_owned()),
                signature: None,
                receiver: None,
                alias: None,
                target_id: None,
                resolution: "unresolved".to_owned(),
                attributes_json: "[\"computed_target\"]".to_owned(),
                limitations_json: "[]".to_owned(),
            },
        ]);
        storage
            .stage_files(generation.clone(), vec![provider, consumer])
            .expect("stage graph files");
        let resolved_edge = StoredResolvedEdge {
            id: "edge-reference".to_owned(),
            relationship: "references".to_owned(),
            source_path: "src/caller.rs".to_owned(),
            source_observation_id: "reference-provide".to_owned(),
            source_category: "reference".to_owned(),
            source_range: ByteRange::new(20, 27).expect("range"),
            source_symbol_id: Some("symbol-caller".to_owned()),
            target: Some(StoredEdgeTarget::LocalSymbol(StoredLocalSymbolRef {
                relative_path: "src/provider.rs".to_owned(),
                symbol_id: "symbol-provider".to_owned(),
            })),
            resolution: "lexically_resolved".to_owned(),
            rule_version: "test-rule-v1".to_owned(),
            resolver_version: "test-resolver-v1".to_owned(),
            candidate_count: 1,
            reason: "test evidence".to_owned(),
            evidence_json: "[]".to_owned(),
            limitations_json: "[]".to_owned(),
        };
        let candidate_edge = StoredResolvedEdge {
            id: "edge-candidate".to_owned(),
            relationship: "calls".to_owned(),
            source_path: "src/caller.rs".to_owned(),
            source_observation_id: "call-provide-candidate".to_owned(),
            source_category: "call_site".to_owned(),
            source_range: ByteRange::new(30, 37).expect("range"),
            source_symbol_id: Some("symbol-caller".to_owned()),
            target: Some(StoredEdgeTarget::LocalSymbol(StoredLocalSymbolRef {
                relative_path: "src/provider.rs".to_owned(),
                symbol_id: "symbol-provider".to_owned(),
            })),
            resolution: "candidate".to_owned(),
            rule_version: "test-rule-v1".to_owned(),
            resolver_version: "test-resolver-v1".to_owned(),
            candidate_count: 2,
            reason: "ambiguous call".to_owned(),
            evidence_json: "[]".to_owned(),
            limitations_json: "[\"ambiguous_candidate_set\"]".to_owned(),
        };
        let unresolved_edge = StoredResolvedEdge {
            id: "edge-unresolved".to_owned(),
            relationship: "calls".to_owned(),
            source_path: "src/caller.rs".to_owned(),
            source_observation_id: "call-dynamic".to_owned(),
            source_category: "call_site".to_owned(),
            source_range: ByteRange::new(40, 50).expect("range"),
            source_symbol_id: Some("symbol-caller".to_owned()),
            target: None,
            resolution: "unresolved".to_owned(),
            rule_version: "test-rule-v1".to_owned(),
            resolver_version: "test-resolver-v1".to_owned(),
            candidate_count: 0,
            reason: "dynamic target".to_owned(),
            evidence_json: "[]".to_owned(),
            limitations_json: "[\"dynamic_property_target_not_resolved\"]".to_owned(),
        };
        let graph = StoredResolutionGraph {
            generation_id: generation.clone(),
            resolver_version: "test-resolver-v1".to_owned(),
            edges: vec![resolved_edge.clone(), candidate_edge, unresolved_edge],
            external_nodes: Vec::new(),
            diagnostics: Vec::new(),
            unresolved_count: 1,
        };
        storage
            .replace_resolution_graph(graph)
            .expect("persist resolved graph");
        let mut stale = resolved_edge;
        stale.id = "edge-stale".to_owned();
        stale.target = Some(StoredEdgeTarget::LocalSymbol(StoredLocalSymbolRef {
            relative_path: "src/deleted.rs".to_owned(),
            symbol_id: "symbol-deleted".to_owned(),
        }));
        assert!(matches!(
            storage.replace_resolution_graph(StoredResolutionGraph {
                generation_id: generation.clone(),
                resolver_version: "test-resolver-v1".to_owned(),
                edges: vec![stale],
                external_nodes: Vec::new(),
                diagnostics: Vec::new(),
                unresolved_count: 0,
            }),
            Err(StorageError::InvalidInput(message)) if message.contains("absent from generation membership")
        ));
        storage
            .complete_generation(GenerationCompletion {
                generation_id: generation.clone(),
                scan_complete: true,
                coverage_status: "syntax_and_name_resolution".to_owned(),
                warning_count: 0,
                failure_summary: None,
            })
            .expect("complete graph generation");
        storage
            .update_job(
                job.id.clone(),
                JobState::Resolving,
                JobProgress::default(),
                None,
            )
            .expect("record resolution");
        storage
            .update_job(job.id, JobState::Committing, JobProgress::default(), None)
            .expect("commit graph generation");
        storage
            .activate_generation(generation)
            .expect("activate graph generation");

        let snapshot = storage.read_snapshot().expect("read graph snapshot");
        assert_eq!(
            snapshot
                .graph_edges(false, 10)
                .expect("default graph")
                .len(),
            2
        );
        assert_eq!(
            snapshot
                .graph_edges(true, 10)
                .expect("candidate graph")
                .len(),
            3
        );
        assert_eq!(
            snapshot
                .adjacent_graph_edges("symbol-provider", StoredGraphDirection::Incoming, false, 10,)
                .expect("incoming graph")
                .len(),
            1
        );
        assert_eq!(
            snapshot
                .call_site_edges("symbol-provider", false, 10)
                .expect("default call sites")
                .len(),
            0
        );
        assert_eq!(
            snapshot
                .call_site_edges("symbol-provider", true, 10)
                .expect("candidate call sites")
                .len(),
            1
        );
        let references = snapshot
            .retrieval_references_to("symbol-provider", true, None, 10)
            .expect("rich references");
        assert_eq!(references.len(), 2);
        assert!(references.iter().any(|reference| {
            reference.resolution == "candidate"
                && reference.candidate_count == 2
                && reference
                    .limitations_json
                    .contains("ambiguous_candidate_set")
        }));
        let relations = snapshot
            .retrieval_adjacent_relations(
                "symbol-provider",
                StoredGraphDirection::Incoming,
                None,
                true,
                10,
            )
            .expect("rich incoming relations");
        assert_eq!(relations.len(), 2);
        assert!(
            relations
                .iter()
                .any(|relation| relation.reason == "ambiguous call")
        );
        let path_relations = snapshot
            .retrieval_incoming_relations_to_path("src/provider.rs", true, 10)
            .expect("path impact relations");
        assert_eq!(path_relations.len(), 2);
        let coverage = snapshot.retrieval_coverage().expect("retrieval coverage");
        assert_eq!(coverage.unresolved_occurrences, 1);
        assert_eq!(coverage.resolver_version, "test-resolver-v1");
        drop(snapshot);

        let deletion_job = storage
            .create_job(NewJob {
                mode: "incremental".to_owned(),
                request_key: None,
                owner_instance_id: "test-owner".to_owned(),
            })
            .expect("create deletion job")
            .job;
        storage
            .update_job(
                deletion_job.id.clone(),
                JobState::Scanning,
                JobProgress::default(),
                None,
            )
            .expect("scan deletion");
        storage
            .update_job(
                deletion_job.id.clone(),
                JobState::Parsing,
                JobProgress::default(),
                None,
            )
            .expect("parse deletion");
        let empty = GenerationId::new("generation-graph-empty").expect("generation ID");
        storage
            .begin_generation(GenerationPlan {
                generation_id: empty.clone(),
                job_id: deletion_job.id.clone(),
                config_hash: "config-v1".to_owned(),
                extractor_set_hash: "extractor-set-v1".to_owned(),
                reuse_parent: false,
                deleted_paths: Vec::new(),
            })
            .expect("begin empty graph generation");
        storage
            .replace_resolution_graph(StoredResolutionGraph {
                generation_id: empty.clone(),
                resolver_version: "test-resolver-v1".to_owned(),
                edges: Vec::new(),
                external_nodes: Vec::new(),
                diagnostics: Vec::new(),
                unresolved_count: 0,
            })
            .expect("resolve empty graph");
        storage
            .complete_generation(GenerationCompletion {
                generation_id: empty.clone(),
                scan_complete: true,
                coverage_status: "syntax_and_name_resolution".to_owned(),
                warning_count: 0,
                failure_summary: None,
            })
            .expect("complete empty graph");
        storage
            .update_job(
                deletion_job.id.clone(),
                JobState::Resolving,
                JobProgress::default(),
                None,
            )
            .expect("resolve deletion");
        storage
            .update_job(
                deletion_job.id,
                JobState::Committing,
                JobProgress::default(),
                None,
            )
            .expect("commit deletion");
        storage
            .activate_generation(empty)
            .expect("activate empty graph");
        assert!(
            storage
                .read_snapshot()
                .expect("empty snapshot")
                .graph_edges(true, 10)
                .expect("empty edges")
                .is_empty()
        );
    }

    #[test]
    fn sqlite_capabilities_fts_backup_and_permissions_are_real() {
        let layout = TestLayout::new("capabilities");
        let storage = layout.open();
        assert_eq!(storage.role(), StorageRole::Owner);
        let capabilities = storage.capabilities().expect("owner capabilities");
        assert!(
            parse_version(&capabilities.version).expect("valid SQLite version")
                >= MIN_SQLITE_VERSION
        );
        assert!(capabilities.fts5);
        assert_eq!(capabilities.journal_mode, "wal");
        assert_eq!(capabilities.schema_version, CURRENT_SCHEMA_VERSION);

        storage
            .put_memory("note-1".to_owned(), "preserved note".to_owned())
            .expect("store memory");
        storage
            .put_memory("note-1".to_owned(), "updated preserved note".to_owned())
            .expect("update memory and its FTS row");
        stage_and_activate(&storage, "generation-1", "alpha", 'a');
        let snapshot = storage.read_snapshot().expect("open active snapshot");
        assert_eq!(snapshot.generation_id().as_str(), "generation-1");
        assert_eq!(snapshot.facts().expect("read facts")[0].name, "alpha");
        assert_eq!(
            snapshot
                .search("alpha", 10)
                .expect("literal FTS search")
                .len(),
            1
        );
        assert!(
            snapshot
                .search("alpha\" OR *", 10)
                .expect("escaped FTS input")
                .is_empty()
        );
        drop(snapshot);
        let memory_reader =
            open_reader_connection(layout.paths.database_path()).expect("open memory FTS reader");
        let memory_hits: i64 = memory_reader
            .query_row(
                "SELECT count(*) FROM memory_fts WHERE memory_fts MATCH 'updated'",
                [],
                |row| row.get(0),
            )
            .expect("query updated memory FTS row");
        assert_eq!(memory_hits, 1);
        drop(memory_reader);

        let backup = layout.base.join("backup.sqlite3");
        storage
            .backup_to(backup.clone())
            .expect("create SQLite backup");
        let backup_connection = Connection::open_with_flags(
            &backup,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .expect("open backup read-only");
        let integrity: String = backup_connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("verify backup integrity");
        assert_eq!(integrity, "ok");
        let note: String = backup_connection
            .query_row("SELECT body FROM memories WHERE id = 'note-1'", [], |row| {
                row.get(0)
            })
            .expect("backup preserved memory");
        assert_eq!(note, "updated preserved note");
        assert!(matches!(
            storage.backup_to(backup),
            Err(StorageError::BackupExists(_))
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(layout.paths.data_directory())
                    .expect("data directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(layout.paths.database_path())
                    .expect("database metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        drop(backup_connection);
        drop(storage);
        let writable = Connection::open(layout.paths.database_path())
            .expect("open direct trigger verification connection");
        writable
            .execute("DELETE FROM search_documents", [])
            .expect("delete external-content source row");
        let stale_fts_rows: i64 = writable
            .query_row(
                "SELECT count(*) FROM symbol_fts WHERE symbol_fts MATCH 'alpha'",
                [],
                |row| row.get(0),
            )
            .expect("verify search delete trigger");
        assert_eq!(stale_fts_rows, 0);
    }

    #[test]
    fn incomplete_generation_never_replaces_active_and_snapshots_stay_pinned() {
        let layout = TestLayout::new("snapshots");
        let storage = layout.open();
        stage_and_activate(&storage, "generation-1", "old_fact", 'a');
        let old_snapshot = storage.read_snapshot().expect("open old snapshot");

        let incomplete = start_generation(&storage, "incomplete", "bad_fact", 'b');
        finish_generation(&storage, "incomplete", &incomplete, false);
        assert!(matches!(
            storage
                .activate_generation(GenerationId::new("incomplete").expect("valid generation ID")),
            Err(StorageError::InvalidGeneration { .. })
        ));
        assert_eq!(
            storage
                .read_snapshot()
                .expect("active generation remains readable")
                .generation_id()
                .as_str(),
            "generation-1"
        );

        stage_and_activate(&storage, "generation-2", "new_fact", 'c');
        assert_eq!(
            old_snapshot.facts().expect("old snapshot facts")[0].name,
            "old_fact"
        );
        let new_snapshot = storage.read_snapshot().expect("open new snapshot");
        assert_eq!(new_snapshot.generation_id().as_str(), "generation-2");
        assert_eq!(
            new_snapshot.facts().expect("new snapshot facts")[0].name,
            "new_fact"
        );
    }

    #[test]
    fn one_writer_lock_allows_read_only_followers_and_releases_on_drop() {
        let layout = TestLayout::new("locking");
        let owner = layout.open();
        stage_and_activate(&owner, "generation-1", "visible", 'a');
        let follower = layout.open();
        assert_eq!(follower.role(), StorageRole::Follower);
        assert_eq!(
            follower
                .read_snapshot()
                .expect("follower snapshot")
                .facts()
                .expect("facts")[0]
                .name,
            "visible"
        );
        assert!(matches!(
            follower.put_memory("blocked".to_owned(), "cannot write".to_owned()),
            Err(StorageError::WriterBusy { .. })
        ));
        assert!(matches!(
            follower.create_job(NewJob {
                mode: "incremental".to_owned(),
                request_key: None,
                owner_instance_id: "follower".to_owned(),
            }),
            Err(StorageError::WriterBusy { .. })
        ));

        drop(follower);
        drop(owner);
        let successor = layout.open();
        assert_eq!(successor.role(), StorageRole::Owner);
    }

    #[test]
    fn second_process_is_a_query_only_follower() {
        let layout = TestLayout::new("process-locking");
        let owner = layout.open();
        stage_and_activate(&owner, "generation-1", "cross_process", 'a');
        let output = Command::new(std::env::current_exe().expect("locate current test binary"))
            .args([
                "--exact",
                "storage::tests::lock_follower_child",
                "--ignored",
            ])
            .env("CODEATLAS_TEST_LOCK_CHILD_ROOT", &layout.root)
            .env(
                "CODEATLAS_TEST_LOCK_CHILD_DATA",
                layout.base.join("application-data"),
            )
            .env(
                "CODEATLAS_TEST_LOCK_CHILD_REPOSITORY_ID",
                layout.repository_id.as_str(),
            )
            .output()
            .expect("run follower child process");
        assert!(
            output.status.success(),
            "follower child failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[ignore = "executed by second_process_is_a_query_only_follower"]
    fn lock_follower_child() {
        let Some(root) = env::var_os("CODEATLAS_TEST_LOCK_CHILD_ROOT") else {
            return;
        };
        let data = env::var_os("CODEATLAS_TEST_LOCK_CHILD_DATA")
            .expect("child data directory environment");
        let repository_id = RepositoryId::new(
            env::var("CODEATLAS_TEST_LOCK_CHILD_REPOSITORY_ID")
                .expect("child repository ID environment"),
        )
        .expect("valid child repository ID");
        let paths = StoragePaths::under(PathBuf::from(data), &repository_id, Path::new(&root))
            .expect("derive child storage paths");
        let follower = Storage::open(repository_id, paths).expect("open child follower");
        assert_eq!(follower.role(), StorageRole::Follower);
        assert_eq!(
            follower
                .read_snapshot()
                .expect("child follower snapshot")
                .facts()
                .expect("child follower facts")[0]
                .name,
            "cross_process"
        );
        assert!(matches!(
            follower.put_memory("blocked".to_owned(), "cross-process".to_owned()),
            Err(StorageError::WriterBusy { .. })
        ));
    }

    #[test]
    fn reopen_marks_interrupted_staging_without_damaging_active_data() {
        let layout = TestLayout::new("recovery");
        {
            let storage = layout.open();
            stage_and_activate(&storage, "generation-1", "healthy", 'a');
            let _unfinished = start_generation(&storage, "generation-2", "unfinished", 'b');
        }
        let reopened = layout.open();
        assert_eq!(
            reopened
                .read_snapshot()
                .expect("healthy active snapshot")
                .generation_id()
                .as_str(),
            "generation-1"
        );
        let connection = open_reader_connection(layout.paths.database_path())
            .expect("open recovery inspection connection");
        let status: String = connection
            .query_row(
                "SELECT status FROM generations WHERE id = 'generation-2'",
                [],
                |row| row.get(0),
            )
            .expect("read recovered generation status");
        let job_status: String = connection
            .query_row(
                "SELECT status FROM jobs WHERE generation_id = 'generation-2'",
                [],
                |row| row.get(0),
            )
            .expect("read recovered job status");
        assert_eq!(status, "abandoned");
        assert_eq!(job_status, "interrupted");
    }

    #[test]
    fn killed_writer_is_recovered_without_displacing_active_data() {
        let layout = TestLayout::new("killed-writer");
        {
            let storage = layout.open();
            stage_and_activate(&storage, "generation-healthy", "healthy", 'a');
            storage
                .put_memory(
                    "crash-note".to_owned(),
                    "preserve across owner death".to_owned(),
                )
                .expect("store crash-recovery note");
        }
        let output = Command::new(std::env::current_exe().expect("locate current test binary"))
            .args([
                "--exact",
                "storage::tests::killed_writer_child",
                "--ignored",
            ])
            .env("CODEATLAS_TEST_KILL_ROOT", &layout.root)
            .env(
                "CODEATLAS_TEST_KILL_DATA",
                layout.base.join("application-data"),
            )
            .env(
                "CODEATLAS_TEST_KILL_REPOSITORY_ID",
                layout.repository_id.as_str(),
            )
            .output()
            .expect("run abruptly exiting writer child");
        assert!(
            output.status.success(),
            "killed-writer child failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let recovered = layout.open();
        assert_eq!(
            recovered
                .read_snapshot()
                .expect("active snapshot survives child exit")
                .generation_id()
                .as_str(),
            "generation-healthy"
        );
        assert_eq!(
            recovered
                .read_memory("crash-note")
                .expect("read note after writer death")
                .as_deref(),
            Some("preserve across owner death")
        );
        let connection = open_reader_connection(layout.paths.database_path())
            .expect("inspect killed-writer recovery");
        let generation_status: String = connection
            .query_row(
                "SELECT status FROM generations WHERE id = 'generation-killed'",
                [],
                |row| row.get(0),
            )
            .expect("read killed generation");
        let job_status: String = connection
            .query_row(
                "SELECT status FROM jobs WHERE generation_id = 'generation-killed'",
                [],
                |row| row.get(0),
            )
            .expect("read killed job");
        assert_eq!(generation_status, "abandoned");
        assert_eq!(job_status, "interrupted");
    }

    #[test]
    #[ignore = "executed by killed_writer_is_recovered_without_displacing_active_data"]
    fn killed_writer_child() {
        let Some(root) = env::var_os("CODEATLAS_TEST_KILL_ROOT") else {
            return;
        };
        let data = env::var_os("CODEATLAS_TEST_KILL_DATA").expect("child data directory");
        let repository_id = RepositoryId::new(
            env::var("CODEATLAS_TEST_KILL_REPOSITORY_ID").expect("child repository ID"),
        )
        .expect("valid child repository ID");
        let paths = StoragePaths::under(PathBuf::from(data), &repository_id, Path::new(&root))
            .expect("derive killed-writer child storage paths");
        let storage = Storage::open(repository_id, paths).expect("open killed-writer child");
        let _unfinished = start_generation(&storage, "generation-killed", "unfinished", 'b');
        std::process::exit(0);
    }

    #[test]
    fn migration_preserves_memories_and_future_schema_is_refused() {
        let layout = TestLayout::new("migration");
        layout.paths.prepare().expect("prepare paths");
        {
            let mut connection = Connection::open(layout.paths.database_path())
                .expect("create version-one database");
            configure_connection(&connection, false).expect("configure database");
            probe_sqlite(&connection).expect("probe SQLite");
            connection
                .pragma_update(None, "journal_mode", "WAL")
                .expect("enable WAL");
            schema::migrate_to(&mut connection, layout.repository_id.as_str(), 1)
                .expect("apply schema version one");
            connection
                .execute(
                    "INSERT INTO memories(id, body, created_at) VALUES ('legacy', 'keep me', 7)",
                    [],
                )
                .expect("insert legacy memory");
        }
        let migrated = layout.open();
        assert_eq!(
            migrated
                .read_memory("legacy")
                .expect("read migrated memory")
                .as_deref(),
            Some("keep me")
        );
        assert_eq!(
            migrated
                .capabilities()
                .expect("capabilities")
                .schema_version,
            CURRENT_SCHEMA_VERSION
        );
        drop(migrated);

        let future = TestLayout::new("future-schema");
        future.paths.prepare().expect("prepare future paths");
        let connection = Connection::open(future.paths.database_path()).expect("create future db");
        connection
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .expect("mark future schema");
        let journal_before: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read future database journal mode");
        drop(connection);
        assert!(matches!(
            Storage::open(future.repository_id.clone(), future.paths.clone()),
            Err(StorageError::FutureSchema { .. })
        ));
        let connection =
            Connection::open(future.paths.database_path()).expect("reopen refused future database");
        let journal_after: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("reread future database journal mode");
        assert_eq!(journal_after, journal_before);
    }

    #[test]
    fn failed_migration_rolls_back_schema_version_and_existing_rows() {
        let layout = TestLayout::new("failed-migration");
        layout.paths.prepare().expect("prepare migration paths");
        {
            let mut connection = Connection::open(layout.paths.database_path())
                .expect("create version-four database");
            configure_connection(&connection, false).expect("configure database");
            probe_sqlite(&connection).expect("probe SQLite");
            schema::migrate_to(&mut connection, layout.repository_id.as_str(), 4)
                .expect("apply schema version four");
            connection
                .execute(
                    "INSERT INTO memories(id, body, created_at) VALUES ('sentinel', 'keep', 1)",
                    [],
                )
                .expect("insert pre-migration row");
            connection
                .execute("ALTER TABLE memories RENAME TO broken_memories", [])
                .expect("create deterministic migration failure");
        }

        assert!(Storage::open(layout.repository_id.clone(), layout.paths.clone()).is_err());
        let connection = Connection::open(layout.paths.database_path())
            .expect("inspect rolled-back migration database");
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version after failed migration");
        assert_eq!(version, 4);
        let sentinel: String = connection
            .query_row(
                "SELECT body FROM broken_memories WHERE id = 'sentinel'",
                [],
                |row| row.get(0),
            )
            .expect("pre-migration data remains");
        assert_eq!(sentinel, "keep");
    }

    #[test]
    fn current_schema_indexes_orphan_file_version_cleanup() {
        let layout = TestLayout::new("orphan-cleanup-index");
        let storage = layout.open();
        assert_eq!(
            storage
                .capabilities()
                .expect("writer capabilities")
                .schema_version,
            CURRENT_SCHEMA_VERSION
        );
        drop(storage);

        let connection =
            Connection::open(layout.paths.database_path()).expect("inspect current schema indexes");
        let index_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_schema
                 WHERE type = 'index' AND name = 'generation_files_file_version_idx'",
                [],
                |row| row.get(0),
            )
            .expect("file-version membership index exists");
        assert!(index_sql.contains("generation_files(file_version_id)"));
        let folded_index_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_schema
                 WHERE type = 'index' AND name = 'symbols_folded_spelling_idx'",
                [],
                |row| row.get(0),
            )
            .expect("folded spelling index exists");
        assert!(folded_index_sql.contains("symbols(lower(spelling))"));
    }

    #[test]
    fn revisioned_memories_validate_evidence_surface_staleness_and_survive_gc() {
        let layout = TestLayout::new("phase-13-memory");
        let storage = layout.open();
        stage_and_activate(&storage, "memory-source-a", "remembered", 'a');
        let evidence = MemoryEvidence {
            relative_path: "src/lib.rs".to_owned(),
            content_hash: "a".repeat(64),
            symbol_id: Some("symbol-memory-source-a".to_owned()),
        };
        let created = storage
            .upsert_memory(MemoryDraft {
                memory_id: Some("decision-1".to_owned()),
                text: "Keep the storage boundary narrow".to_owned(),
                kind: MemoryKind::Decision,
                author: "test-user".to_owned(),
                origin: "unit-test".to_owned(),
                scope: "repository".to_owned(),
                evidence: vec![evidence.clone()],
                expected_revision: None,
            })
            .expect("create evidence-backed memory");
        assert_eq!(created.revision, 1);
        assert_eq!(created.evidence_status, MemoryEvidenceStatus::Verified);
        assert!(matches!(
            storage.upsert_memory(MemoryDraft {
                memory_id: Some("decision-1".to_owned()),
                text: "unversioned overwrite".to_owned(),
                kind: MemoryKind::Decision,
                author: "test-user".to_owned(),
                origin: "unit-test".to_owned(),
                scope: "repository".to_owned(),
                evidence: vec![evidence.clone()],
                expected_revision: None,
            }),
            Err(StorageError::MemoryRevisionRequired { .. })
        ));
        assert!(matches!(
            storage.upsert_memory(MemoryDraft {
                memory_id: Some("decision-1".to_owned()),
                text: "conflicting overwrite".to_owned(),
                kind: MemoryKind::Decision,
                author: "test-user".to_owned(),
                origin: "unit-test".to_owned(),
                scope: "repository".to_owned(),
                evidence: vec![evidence.clone()],
                expected_revision: Some(9),
            }),
            Err(StorageError::MemoryRevisionConflict { .. })
        ));
        let updated = storage
            .upsert_memory(MemoryDraft {
                memory_id: Some("decision-1".to_owned()),
                text: "Keep the storage boundary narrow and explicit".to_owned(),
                kind: MemoryKind::Decision,
                author: "test-user".to_owned(),
                origin: "unit-test".to_owned(),
                scope: "repository".to_owned(),
                evidence: vec![evidence],
                expected_revision: Some(1),
            })
            .expect("revision-checked update");
        assert_eq!(updated.revision, 2);
        assert_eq!(
            storage
                .search_memories(&MemorySearch {
                    query: "storage boundary".to_owned(),
                    scope: None,
                    include_stale: false,
                    limit: 10,
                })
                .expect("search current memories")
                .len(),
            1
        );

        stage_and_activate(&storage, "memory-source-b", "replacement", 'b');
        storage.gc_abandoned().expect("GC keeps memories");
        let stale = storage
            .memory("decision-1")
            .expect("read stale memory")
            .expect("memory remains after reindex and GC");
        assert_eq!(stale.evidence_status, MemoryEvidenceStatus::Stale);
        assert!(
            storage
                .search_memories(&MemorySearch {
                    query: "storage boundary".to_owned(),
                    scope: None,
                    include_stale: false,
                    limit: 10,
                })
                .expect("exclude stale memories")
                .is_empty()
        );
        assert_eq!(
            storage
                .search_memories(&MemorySearch {
                    query: "storage boundary".to_owned(),
                    scope: None,
                    include_stale: true,
                    limit: 10,
                })
                .expect("include stale memories")
                .len(),
            1
        );
        assert!(matches!(
            storage.forget_memory("decision-1".to_owned(), 1),
            Err(StorageError::MemoryRevisionConflict { .. })
        ));
        let deleted = storage
            .forget_memory("decision-1".to_owned(), 2)
            .expect("delete at exact revision");
        assert_eq!(deleted.revision, 2);
        assert!(
            storage
                .memory("decision-1")
                .expect("read deletion")
                .is_none()
        );
    }

    #[test]
    fn corrupt_databases_and_invalid_backup_destinations_surface_errors() {
        let corrupt = TestLayout::new("corrupt");
        corrupt.paths.prepare().expect("prepare corrupt paths");
        let original = b"not a sqlite database";
        fs::write(corrupt.paths.database_path(), original).expect("write corrupt database");
        assert!(Storage::open(corrupt.repository_id.clone(), corrupt.paths.clone()).is_err());
        assert_eq!(
            fs::read(corrupt.paths.database_path()).expect("corrupt file remains inspectable"),
            original
        );

        let layout = TestLayout::new("backup-error");
        let storage = layout.open();
        let missing_parent = layout.base.join("missing").join("backup.sqlite3");
        assert!(matches!(
            storage.backup_to(missing_parent),
            Err(StorageError::InvalidInput(_))
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let read_only = layout.base.join("read-only");
            fs::create_dir(&read_only).expect("create read-only backup directory");
            fs::set_permissions(&read_only, fs::Permissions::from_mode(0o500))
                .expect("make backup directory read-only");
            let result = storage.backup_to(read_only.join("backup.sqlite3"));
            fs::set_permissions(&read_only, fs::Permissions::from_mode(0o700))
                .expect("restore backup directory permissions");
            assert!(matches!(result, Err(StorageError::Io { .. })));
        }
    }

    #[test]
    fn storage_paths_reject_source_tree_and_relative_data_locations() {
        let layout = TestLayout::new("path-policy");
        let other_id = RepositoryId::new("other-worktree").expect("valid other ID");
        let other_paths = StoragePaths::under(
            layout.base.join("application-data"),
            &other_id,
            &layout.root,
        )
        .expect("derive second worktree paths");
        assert_ne!(layout.paths.database_path(), other_paths.database_path());
        assert!(matches!(
            StoragePaths::under("relative", &layout.repository_id, &layout.root),
            Err(StorageError::DataDirectoryNotAbsolute(_))
        ));
        assert!(matches!(
            StoragePaths::under(
                layout.root.join("nested-data"),
                &layout.repository_id,
                &layout.root
            ),
            Err(StorageError::DataDirectoryInsideRoot(_))
        ));
    }

    #[test]
    fn pinned_retrieval_search_is_ranked_bounded_and_literal_safe() {
        let layout = TestLayout::new("retrieval-search");
        let storage = layout.open();
        let mut file = staged_file("generation-search", "getHTTPServer", 'a');
        file.observations[0].container = Some("Service".to_owned());
        let job = start_generation_with_file(&storage, "generation-search", file);
        finish_generation(&storage, "generation-search", &job, true);
        storage
            .activate_generation(
                GenerationId::new("generation-search").expect("valid generation ID"),
            )
            .expect("activate search generation");
        let snapshot = storage.read_snapshot().expect("open pinned snapshot");
        assert_eq!(snapshot.repository_id(), &layout.repository_id);
        let exact = snapshot
            .retrieval_search_symbols(&StoredSearchPlan {
                query: "getHTTPServer".to_owned(),
                folded_query: "gethttpserver".to_owned(),
                escaped_folded_prefix: "gethttpserver%".to_owned(),
                fts_expression: "\"getHTTPServer\"".to_owned(),
                language: Some("rust".to_owned()),
                kind: Some("function".to_owned()),
                path_prefix: Some("src".to_owned()),
                after: None,
                fetch_limit: 2,
            })
            .expect("rank exact search");
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].tier, 1);
        assert_eq!(exact[0].symbol.name, "getHTTPServer");
        assert_eq!(exact[0].symbol.relative_path, "src/lib.rs");

        let qualified = snapshot
            .retrieval_search_symbols(&StoredSearchPlan {
                query: "Service::getHTTPServer".to_owned(),
                folded_query: "service::gethttpserver".to_owned(),
                escaped_folded_prefix: "service::gethttpserver%".to_owned(),
                fts_expression: "\"Service\" AND \"getHTTPServer\"".to_owned(),
                language: None,
                kind: None,
                path_prefix: None,
                after: None,
                fetch_limit: 2,
            })
            .expect("rank qualified search");
        assert_eq!(qualified[0].tier, 0);
        assert_eq!(qualified[0].symbol.container.as_deref(), Some("Service"));

        let punctuation = snapshot
            .retrieval_search_symbols(&StoredSearchPlan {
                query: "::".to_owned(),
                folded_query: "::".to_owned(),
                escaped_folded_prefix: "::%".to_owned(),
                fts_expression: "\"__codeatlas_no_match__\"".to_owned(),
                language: None,
                kind: None,
                path_prefix: None,
                after: None,
                fetch_limit: 2,
            })
            .expect("treat punctuation as literal input");
        assert!(punctuation.is_empty());
        assert!(
            snapshot
                .retrieval_search_symbols(&StoredSearchPlan {
                    query: "x".to_owned(),
                    folded_query: "x".to_owned(),
                    escaped_folded_prefix: "x%".to_owned(),
                    fts_expression: "x OR *".to_owned(),
                    language: None,
                    kind: None,
                    path_prefix: None,
                    after: None,
                    fetch_limit: 1,
                })
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn storage_paths_and_database_files_reject_symlink_redirection() {
        use std::os::unix::fs::symlink;

        let layout = TestLayout::new("storage-links");
        let redirected_base = layout.base.join("redirected-data");
        fs::create_dir(&redirected_base).expect("create redirected data base");
        let application_name = if cfg!(target_os = "macos") {
            "CodeAtlas"
        } else {
            "codeatlas"
        };
        symlink(&layout.root, redirected_base.join(application_name))
            .expect("create application-data symlink");
        assert!(matches!(
            StoragePaths::under(&redirected_base, &layout.repository_id, &layout.root),
            Err(StorageError::DataDirectoryInsideRoot(_))
        ));

        layout.paths.prepare().expect("prepare storage paths");
        let outside = layout.base.join("outside.sqlite3");
        fs::write(&outside, b"outside").expect("write outside target");
        symlink(&outside, layout.paths.database_path()).expect("create database symlink");
        assert!(matches!(
            Storage::open(layout.repository_id.clone(), layout.paths.clone()),
            Err(StorageError::UnsafePath(_))
        ));
    }
}
