use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use ca_core::RepositoryRoot;
use thiserror::Error;
use toml_edit::{Array, DocumentMut, Item, Table, value};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const OWNED_MARKER: &str = "# codeatlas-owned: v1";

#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) root: Option<PathBuf>,
    pub(crate) binary: Option<PathBuf>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) remove: bool,
    pub(crate) apply: bool,
}

#[derive(Debug)]
pub(crate) struct ResultSummary {
    pub(crate) config: PathBuf,
    pub(crate) backup: Option<PathBuf>,
    pub(crate) diff: String,
}

#[derive(Debug, Error)]
pub(crate) enum CodexIntegrationError {
    #[error("{kind} path must be absolute: {path}")]
    RelativePath { kind: &'static str, path: String },
    #[error("--root is required when installing the Codex integration")]
    MissingRoot,
    #[error("cannot determine the current executable: {0}")]
    Executable(#[source] io::Error),
    #[error("cannot determine the Codex config path; pass --config or set CODEX_HOME/HOME")]
    ConfigHome,
    #[error("cannot inspect {path}: {source}")]
    Inspect { path: String, source: io::Error },
    #[error("Codex config path is a symbolic link; refusing to replace it: {0}")]
    ConfigSymlink(String),
    #[error("cannot read Codex config {path}: {source}")]
    Read { path: String, source: io::Error },
    #[error("malformed Codex config {path} at line {line}, column {column}")]
    Malformed {
        path: String,
        line: usize,
        column: usize,
    },
    #[error("mcp_servers exists but is not a TOML table; refusing to replace it")]
    McpServersCollision,
    #[error("mcp_servers.codeatlas already exists and is not owned by CodeAtlas")]
    NameCollision,
    #[error("no CodeAtlas-owned mcp_servers.codeatlas entry exists")]
    NotInstalled,
    #[error("cannot canonicalize {kind} path {path}: {source}")]
    Canonicalize {
        kind: &'static str,
        path: String,
        source: io::Error,
    },
    #[error("CodeAtlas binary is not a regular file: {0}")]
    BinaryNotFile(String),
    #[error("CodeAtlas binary is not executable: {0}")]
    BinaryNotExecutable(String),
    #[error("cannot create or replace Codex config {path}: {source}")]
    Write { path: String, source: io::Error },
}

pub(crate) fn run(request: Request) -> Result<ResultSummary, CodexIntegrationError> {
    let config = config_path(request.config)?;
    reject_config_symlink(&config)?;
    let original = read_optional(&config)?;
    let mut document = parse_document(&config, original.as_deref().unwrap_or(b""))?;
    let before = owned_entry(&document)?.map(render_entry);

    if request.remove {
        remove_owned_entry(&mut document)?;
    } else {
        let root = request.root.ok_or(CodexIntegrationError::MissingRoot)?;
        let root = canonical_root(root)?;
        let binary = canonical_binary(request.binary)?;
        install_owned_entry(&mut document, &binary, root.as_path())?;
    }

    let after = owned_entry(&document)?.map(render_entry);
    let diff = entry_diff(&config, before.as_deref(), after.as_deref());
    let proposed = document.to_string();
    let backup = if request.apply {
        write_atomic_with_backup(&config, original.as_deref(), proposed.as_bytes())?
    } else {
        None
    };
    Ok(ResultSummary {
        config,
        backup,
        diff,
    })
}

fn config_path(explicit: Option<PathBuf>) -> Result<PathBuf, CodexIntegrationError> {
    let path = match explicit {
        Some(path) => path,
        None => {
            if let Some(home) = env::var_os("CODEX_HOME") {
                PathBuf::from(home).join("config.toml")
            } else if let Some(home) = env::var_os("HOME") {
                PathBuf::from(home).join(".codex/config.toml")
            } else {
                return Err(CodexIntegrationError::ConfigHome);
            }
        }
    };
    require_absolute("config", path)
}

fn require_absolute(kind: &'static str, path: PathBuf) -> Result<PathBuf, CodexIntegrationError> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(CodexIntegrationError::RelativePath {
            kind,
            path: path.display().to_string(),
        })
    }
}

fn reject_config_symlink(path: &Path) -> Result<(), CodexIntegrationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(
            CodexIntegrationError::ConfigSymlink(path.display().to_string()),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CodexIntegrationError::Inspect {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, CodexIntegrationError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CodexIntegrationError::Read {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn parse_document(path: &Path, bytes: &[u8]) -> Result<DocumentMut, CodexIntegrationError> {
    let source = std::str::from_utf8(bytes).map_err(|error| CodexIntegrationError::Read {
        path: path.display().to_string(),
        source: io::Error::new(io::ErrorKind::InvalidData, error),
    })?;
    source.parse::<DocumentMut>().map_err(|error| {
        let offset = error.span().map_or(0, |span| span.start).min(source.len());
        let prefix = &source.as_bytes()[..offset];
        CodexIntegrationError::Malformed {
            path: path.display().to_string(),
            line: prefix.iter().filter(|byte| **byte == b'\n').count() + 1,
            column: prefix
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(offset + 1, |newline| offset - newline),
        }
    })
}

fn canonical_root(path: PathBuf) -> Result<RepositoryRoot, CodexIntegrationError> {
    let path = require_absolute("root", path)?;
    RepositoryRoot::new(path.clone()).map_err(|error| CodexIntegrationError::Canonicalize {
        kind: "root",
        path: path.display().to_string(),
        source: io::Error::new(io::ErrorKind::InvalidInput, error),
    })
}

fn canonical_binary(explicit: Option<PathBuf>) -> Result<PathBuf, CodexIntegrationError> {
    let path = match explicit {
        Some(path) => require_absolute("binary", path)?,
        None => env::current_exe().map_err(CodexIntegrationError::Executable)?,
    };
    let canonical = path
        .canonicalize()
        .map_err(|source| CodexIntegrationError::Canonicalize {
            kind: "binary",
            path: path.display().to_string(),
            source,
        })?;
    if !canonical.is_file() {
        return Err(CodexIntegrationError::BinaryNotFile(
            canonical.display().to_string(),
        ));
    }
    #[cfg(unix)]
    if fs::metadata(&canonical)
        .map_err(|source| CodexIntegrationError::Inspect {
            path: canonical.display().to_string(),
            source,
        })?
        .permissions()
        .mode()
        & 0o111
        == 0
    {
        return Err(CodexIntegrationError::BinaryNotExecutable(
            canonical.display().to_string(),
        ));
    }
    Ok(canonical)
}

fn owned_entry(document: &DocumentMut) -> Result<Option<&Table>, CodexIntegrationError> {
    let Some(servers) = document.get("mcp_servers") else {
        return Ok(None);
    };
    let servers = servers
        .as_table()
        .ok_or(CodexIntegrationError::McpServersCollision)?;
    let Some(entry) = servers.get("codeatlas") else {
        return Ok(None);
    };
    let table = entry
        .as_table()
        .ok_or(CodexIntegrationError::NameCollision)?;
    if is_owned(table) {
        Ok(Some(table))
    } else {
        Err(CodexIntegrationError::NameCollision)
    }
}

fn is_owned(table: &Table) -> bool {
    table
        .decor()
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .is_some_and(|prefix| prefix.contains(OWNED_MARKER))
}

fn install_owned_entry(
    document: &mut DocumentMut,
    binary: &Path,
    root: &Path,
) -> Result<(), CodexIntegrationError> {
    let _ = owned_entry(document)?;
    if document.get("mcp_servers").is_none() {
        document.insert("mcp_servers", Item::Table(Table::new()));
    }
    let servers = document
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
        .ok_or(CodexIntegrationError::McpServersCollision)?;

    let mut args = Array::new();
    args.push("serve");
    args.push("--root");
    args.push(root.to_string_lossy().into_owned());
    let mut entry = Table::new();
    entry.decor_mut().set_prefix(format!("\n{OWNED_MARKER}\n"));
    entry.insert("command", value(binary.to_string_lossy().into_owned()));
    entry.insert("args", value(args));
    entry.insert("required", value(false));
    entry.insert("startup_timeout_sec", value(10));
    entry.insert("tool_timeout_sec", value(60));
    servers.insert("codeatlas", Item::Table(entry));
    Ok(())
}

fn remove_owned_entry(document: &mut DocumentMut) -> Result<(), CodexIntegrationError> {
    if owned_entry(document)?.is_none() {
        return Err(CodexIntegrationError::NotInstalled);
    }
    let servers = document
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
        .ok_or(CodexIntegrationError::McpServersCollision)?;
    let _ = servers.remove("codeatlas");
    Ok(())
}

fn render_entry(table: &Table) -> String {
    let mut document = DocumentMut::new();
    let mut servers = Table::new();
    servers.insert("codeatlas", Item::Table(table.clone()));
    document.insert("mcp_servers", Item::Table(servers));
    document.to_string()
}

fn entry_diff(config: &Path, before: Option<&str>, after: Option<&str>) -> String {
    let mut output = format!(
        "--- {} (current CodeAtlas entry)\n+++ {} (proposed CodeAtlas entry)\n@@ mcp_servers.codeatlas @@\n",
        config.display(),
        config.display()
    );
    for line in before.unwrap_or("<entry absent>\n").lines() {
        output.push('-');
        output.push_str(line);
        output.push('\n');
    }
    for line in after.unwrap_or("<entry absent>\n").lines() {
        output.push('+');
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn write_atomic_with_backup(
    path: &Path,
    original: Option<&[u8]>,
    proposed: &[u8],
) -> Result<Option<PathBuf>, CodexIntegrationError> {
    let parent = path.parent().ok_or_else(|| CodexIntegrationError::Write {
        path: path.display().to_string(),
        source: io::Error::new(io::ErrorKind::InvalidInput, "config has no parent"),
    })?;
    fs::create_dir_all(parent).map_err(|source| CodexIntegrationError::Write {
        path: parent.display().to_string(),
        source,
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let base = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let backup = if let Some(bytes) = original {
        let backup = parent.join(format!("{base}.codeatlas-backup-{nonce}"));
        write_new(&backup, bytes)?;
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&backup, metadata.permissions()).map_err(|source| {
                CodexIntegrationError::Write {
                    path: backup.display().to_string(),
                    source,
                }
            })?;
        }
        Some(backup)
    } else {
        None
    };

    let temporary = parent.join(format!(
        ".{base}.codeatlas-tmp-{}-{nonce}",
        std::process::id()
    ));
    if let Err(source) = write_new(&temporary, proposed) {
        let _ = fs::remove_file(&temporary);
        return Err(source);
    }
    if let Some(metadata) = original.and_then(|_| fs::metadata(path).ok())
        && let Err(source) = fs::set_permissions(&temporary, metadata.permissions())
    {
        let _ = fs::remove_file(&temporary);
        return Err(CodexIntegrationError::Write {
            path: temporary.display().to_string(),
            source,
        });
    }
    if let Err(source) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(CodexIntegrationError::Write {
            path: path.display().to_string(),
            source,
        });
    }
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(backup)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), CodexIntegrationError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|source| CodexIntegrationError::Write {
            path: path.display().to_string(),
            source,
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| CodexIntegrationError::Write {
            path: path.display().to_string(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        directory: PathBuf,
        root: PathBuf,
        binary: PathBuf,
        config: PathBuf,
        notes: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = env::temp_dir().join(format!(
                "codeatlas integration ğ-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&directory).expect("create test directory");
            let root = directory.join("source root");
            fs::create_dir(&root).expect("create source root");
            fs::write(root.join("keep.rs"), "fn keep() {}\n").expect("write source");
            let binary = directory.join("codeatlas binary");
            fs::write(&binary, "native executable placeholder").expect("write binary");
            #[cfg(unix)]
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
                .expect("make test binary executable");
            let config = directory.join("Codex Home/config.toml");
            fs::create_dir_all(config.parent().expect("config parent")).expect("create config");
            let notes = directory.join("application-data/notes.sqlite");
            fs::create_dir_all(notes.parent().expect("notes parent")).expect("create notes parent");
            fs::write(&notes, "preserve-notes").expect("write notes");
            Self {
                directory,
                root,
                binary,
                config,
                notes,
            }
        }

        fn request(&self, remove: bool, apply: bool) -> Request {
            Request {
                root: (!remove).then(|| self.root.clone()),
                binary: (!remove).then(|| self.binary.clone()),
                config: Some(self.config.clone()),
                remove,
                apply,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn dry_run_is_surgical_and_does_not_write() {
        let fixture = Fixture::new();
        let original = "# keep this comment\nmodel = \"gpt-test\"\n\n[mcp_servers.other]\ncommand = \"other\"\n";
        fs::write(&fixture.config, original).expect("write config");
        let result = run(fixture.request(false, false)).expect("dry run");
        assert_eq!(
            fs::read_to_string(&fixture.config).expect("read config"),
            original
        );
        assert!(result.diff.contains("required = false"));
        assert!(result.diff.contains(&fixture.binary.display().to_string()));
        assert!(!result.diff.contains("model = \"gpt-test\""));
    }

    #[test]
    fn apply_preserves_unrelated_content_and_creates_backup() {
        let fixture = Fixture::new();
        let original =
            "# preserve\nmodel = \"gpt-test\"\n\n[mcp_servers.other]\ncommand = \"other\"\n";
        fs::write(&fixture.config, original).expect("write config");
        let result = run(fixture.request(false, true)).expect("apply");
        let updated = fs::read_to_string(&fixture.config).expect("read config");
        assert!(updated.starts_with(original));
        assert!(updated.contains(OWNED_MARKER));
        assert!(updated.contains("required = false"));
        let backup = result.backup.expect("backup");
        assert_eq!(fs::read_to_string(backup).expect("read backup"), original);
    }

    #[test]
    fn collision_and_malformed_toml_are_unchanged() {
        let fixture = Fixture::new();
        let collision = "[mcp_servers.codeatlas]\ncommand = \"somebody-else\"\n";
        fs::write(&fixture.config, collision).expect("write collision");
        assert!(matches!(
            run(fixture.request(false, true)),
            Err(CodexIntegrationError::NameCollision)
        ));
        assert_eq!(
            fs::read_to_string(&fixture.config).expect("read collision"),
            collision
        );

        let malformed = "[mcp_servers\ncommand =";
        fs::write(&fixture.config, malformed).expect("write malformed");
        assert!(matches!(
            run(fixture.request(false, true)),
            Err(CodexIntegrationError::Malformed { .. })
        ));
        assert_eq!(
            fs::read_to_string(&fixture.config).expect("read malformed"),
            malformed
        );
    }

    #[test]
    fn malformed_config_diagnostics_never_include_private_source_or_error_chain() {
        let fixture = Fixture::new();
        let canary = "PRIVATE_CONFIG_CANARY_81ab";
        let original = format!("api_key = \"{canary}\" invalid\n");
        fs::write(&fixture.config, &original).expect("private malformed config");
        for remove in [false, true] {
            for apply in [false, true] {
                let error =
                    run(fixture.request(remove, apply)).expect_err("reject malformed config");
                assert!(!format!("{error:?} {error}").contains(canary));
                assert!(std::error::Error::source(&error).is_none());
                assert_eq!(
                    fs::read_to_string(&fixture.config).expect("unchanged config"),
                    original
                );
            }
        }
    }

    #[test]
    fn remove_deletes_only_owned_entry_and_preserves_source_and_notes() {
        let fixture = Fixture::new();
        fs::write(
            &fixture.config,
            "# global\nmodel = \"gpt-test\"\n\n[mcp_servers.other]\ncommand = \"other\"\n",
        )
        .expect("write config");
        run(fixture.request(false, true)).expect("install");
        let result = run(fixture.request(true, true)).expect("remove");
        let updated = fs::read_to_string(&fixture.config).expect("read config");
        assert!(updated.contains("# global"));
        assert!(updated.contains("[mcp_servers.other]"));
        assert!(!updated.contains("codeatlas-owned"));
        assert_eq!(
            fs::read_to_string(fixture.root.join("keep.rs")).expect("read source"),
            "fn keep() {}\n"
        );
        assert_eq!(
            fs::read_to_string(&fixture.notes).expect("read notes"),
            "preserve-notes"
        );
        assert!(result.backup.is_some());
    }
}
