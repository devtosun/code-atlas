#![forbid(unsafe_code)]

mod codex_integration;
mod indexer;
mod mcp_backend;
mod memory;
pub mod retrieval;
mod watcher;

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use ca_core::{CancellationContext, CoreError, RepositoryRoot};
use ca_engine::{
    indexing::{IndexLimits, IndexMode, IndexRequest, IndexService, IndexingError},
    repository::{RepositoryError, RepositoryIdentity},
    repository::{ScanPolicy, SourceReader},
};
use ca_storage::{CURRENT_SCHEMA_VERSION, IndexSummary, Storage, StorageError, StoragePaths};
use clap::{Parser, Subcommand};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(
    name = "codeatlas",
    version,
    about = "Local-first code intelligence over MCP",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the MCP server over stdin/stdout.
    Serve {
        /// Absolute repository root authorized for this process.
        #[arg(long, value_name = "ABSOLUTE_PATH")]
        root: PathBuf,
        /// Keep an existing index fresh while this writer-owner session is alive.
        #[arg(long, default_value_t = false)]
        watch: bool,
        /// Quiet period used to coalesce filesystem hints.
        #[arg(long, default_value_t = 250, value_name = "MILLISECONDS")]
        watch_debounce_ms: u64,
        /// Interval for authoritative scan/hash reconciliation.
        #[arg(long, default_value_t = 60, value_name = "SECONDS")]
        watch_reconcile_seconds: u64,
        /// Use content-comparing polling instead of the native notification backend.
        #[arg(long, default_value_t = false)]
        watch_poll: bool,
        /// Allow explicit project-memory writes for this trusted server process.
        #[arg(long, default_value_t = false)]
        memory_write: bool,
    },
    /// Report runtime, storage, parser availability and lock health.
    Doctor {
        /// Repository root to validate; defaults to the current directory.
        #[arg(long, value_name = "ABSOLUTE_PATH")]
        root: Option<PathBuf>,
        /// Emit stable machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Build and atomically activate a bounded persistent syntax index.
    Index {
        /// Absolute repository root authorized for this process.
        #[arg(long, value_name = "ABSOLUTE_PATH")]
        root: PathBuf,
        /// Parse every supported source file instead of reusing unchanged versions.
        #[arg(long)]
        full: bool,
        /// Optional idempotency key scoped to this repository's index operation.
        #[arg(long, value_name = "KEY")]
        request_key: Option<String>,
        /// Emit stable machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Inspect the active generation and latest durable index job.
    Status {
        /// Absolute repository root authorized for this process.
        #[arg(long, value_name = "ABSOLUTE_PATH")]
        root: PathBuf,
        /// Emit stable machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Safely add or remove the owned CodeAtlas entry in Codex configuration.
    Integrate {
        #[command(subcommand)]
        target: IntegrationTarget,
    },
}

#[derive(Debug, Subcommand)]
enum IntegrationTarget {
    /// Integrate this native binary with Codex without changing global PATH.
    Codex {
        /// Absolute repository root authorized for this Codex server.
        #[arg(long, value_name = "ABSOLUTE_PATH")]
        root: Option<PathBuf>,
        /// Absolute CodeAtlas executable; defaults to the running executable.
        #[arg(long, value_name = "ABSOLUTE_PATH")]
        binary: Option<PathBuf>,
        /// Absolute Codex TOML path; defaults to $CODEX_HOME/config.toml or ~/.codex/config.toml.
        #[arg(long, value_name = "ABSOLUTE_PATH")]
        config: Option<PathBuf>,
        /// Remove only an entry previously created by CodeAtlas.
        #[arg(long)]
        remove: bool,
        /// Print the proposed entry-level diff without writing.
        #[arg(long, conflicts_with = "apply", required_unless_present = "apply")]
        dry_run: bool,
        /// Back up the current file and atomically apply the proposed change.
        #[arg(long, conflicts_with = "dry_run", required_unless_present = "dry_run")]
        apply: bool,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Indexing(#[from] IndexingError),
    #[error("cannot determine the current directory: {0}")]
    CurrentDirectory(#[source] std::io::Error),
    #[error("cannot determine the executable path: {0}")]
    ExecutablePath(#[source] std::io::Error),
    #[error("cannot initialize stderr tracing: {0}")]
    Tracing(String),
    #[error(transparent)]
    Mcp(#[from] ca_mcp::McpServerError),
    #[error(transparent)]
    Watch(#[from] watcher::WatchError),
    #[error(transparent)]
    CodexIntegration(#[from] codex_integration::CodexIntegrationError),
    #[error("cannot encode CLI output: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    schema_version: u32,
    binary: BinaryFacts,
    runtime: RuntimeFacts,
    configuration: ConfigurationFacts,
    capabilities: CapabilityFacts,
}

#[derive(Debug, Serialize)]
struct BinaryFacts {
    name: &'static str,
    version: &'static str,
    path: String,
    rustc: &'static str,
    target: &'static str,
}

#[derive(Debug, Serialize)]
struct RuntimeFacts {
    os: &'static str,
    architecture: &'static str,
}

#[derive(Debug, Serialize)]
struct ConfigurationFacts {
    source: &'static str,
    repository_root: String,
    repository_id: String,
    repository_kind: &'static str,
    git_directory: Option<String>,
    common_git_directory: Option<String>,
    data_directory: String,
    database_path: String,
    storage_permissions: &'static str,
}

#[derive(Debug, Serialize)]
struct CapabilityFacts {
    repository_lifecycle: &'static str,
    database: &'static str,
    storage_role: &'static str,
    sqlite_version: Option<String>,
    fts5: Option<bool>,
    parsers: &'static str,
    startup_network: bool,
    protocols: [&'static str; 2],
    tools: [&'static str; 14],
    resources: [&'static str; 4],
    prompts: [&'static str; 3],
    memory_writes: bool,
}

fn init_serve_tracing() -> Result<(), CliError> {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|error| CliError::Tracing(error.to_string()))
}

fn resolve_root(root: Option<PathBuf>) -> Result<RepositoryRoot, CliError> {
    let path = match root {
        Some(path) => path,
        None => std::env::current_dir().map_err(CliError::CurrentDirectory)?,
    };
    RepositoryRoot::new(path).map_err(CliError::from)
}

fn open_storage(root: &RepositoryRoot) -> Result<(RepositoryIdentity, Storage), CliError> {
    let identity = RepositoryIdentity::derive(root.clone())?;
    let storage_paths = StoragePaths::platform(identity.id(), identity.root().as_path())?;
    let storage = Storage::open(identity.id().clone(), storage_paths)?;
    Ok((identity, storage))
}

fn doctor_report(root: &RepositoryRoot) -> Result<DoctorReport, CliError> {
    let executable = std::env::current_exe().map_err(CliError::ExecutablePath)?;
    let (identity, storage) = open_storage(root)?;
    let storage_paths = storage.paths();
    let capabilities = storage.capabilities();
    Ok(DoctorReport {
        schema_version: CURRENT_SCHEMA_VERSION,
        binary: BinaryFacts {
            name: "codeatlas",
            version: env!("CARGO_PKG_VERSION"),
            path: executable.display().to_string(),
            rustc: env!("CODEATLAS_BUILD_RUSTC"),
            target: env!("CODEATLAS_BUILD_TARGET"),
        },
        runtime: RuntimeFacts {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
        },
        configuration: ConfigurationFacts {
            source: "command_line_or_current_directory",
            repository_root: identity.root().as_utf8().to_owned(),
            repository_id: identity.id().as_str().to_owned(),
            repository_kind: identity.kind().as_str(),
            git_directory: identity.git_dir().map(|path| path.display().to_string()),
            common_git_directory: identity
                .common_git_dir()
                .map(|path| path.display().to_string()),
            data_directory: storage_paths.data_directory().display().to_string(),
            database_path: storage_paths.database_path().display().to_string(),
            storage_permissions: StoragePaths::permission_policy(),
        },
        capabilities: CapabilityFacts {
            repository_lifecycle: "opened",
            database: "ready",
            storage_role: storage.role().as_str(),
            sqlite_version: capabilities.map(|value| value.version.clone()),
            fts5: capabilities.map(|value| value.fts5),
            parsers: "available_on_index",
            startup_network: false,
            protocols: ["2026-07-28", "2025-11-25"],
            tools: [
                "repository_status",
                "index_repository",
                "job_status",
                "cancel_job",
                "search_symbols",
                "get_symbol",
                "find_references",
                "trace_calls",
                "get_file_outline",
                "read_code",
                "get_repo_map",
                "analyze_impact",
                "build_context",
                "search_memories",
            ],
            resources: [
                "codeatlas://repo/status",
                "codeatlas://repo/map",
                "codeatlas://repo/symbol/{id}",
                "codeatlas://repo/memory/{id}",
            ],
            prompts: ["explain_symbol", "plan_change", "investigate_failure"],
            memory_writes: false,
        },
    })
}

#[derive(Debug, Serialize)]
struct IndexReport {
    schema_version: u32,
    job_id: String,
    generation_id: String,
    state: &'static str,
    deduplicated: bool,
    files_discovered: u64,
    files_reused: u64,
    files_parsed: u64,
    files_failed: u64,
    files_persisted: u64,
    files_deleted: u64,
    warning_count: u64,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StatusReport {
    schema_version: u32,
    repository_root: String,
    repository_id: String,
    storage_role: &'static str,
    active_generation_id: Option<String>,
    active_files: u64,
    active_facts: u64,
    active_symbols: u64,
    latest_job: Option<JobReport>,
}

#[derive(Debug, Serialize)]
struct JobReport {
    id: String,
    generation_id: Option<String>,
    mode: String,
    state: &'static str,
    cancel_requested: bool,
    files_discovered: u64,
    files_reused: u64,
    files_parsed: u64,
    files_failed: u64,
    files_persisted: u64,
    files_deleted: u64,
    warning_count: u64,
    error_summary: Option<String>,
}

fn run_index(
    root: RepositoryRoot,
    full: bool,
    request_key: Option<String>,
) -> Result<IndexReport, CliError> {
    let (identity, storage) = open_storage(&root)?;
    let reader = SourceReader::new(identity, ScanPolicy::default());
    let adapter = indexer::StorageIndexAdapter::new(&storage);
    let factory = indexer::ParserFactory;
    let service = IndexService::new(reader, &adapter, &factory, IndexLimits::default())?;
    let outcome = service.run(
        IndexRequest {
            mode: if full {
                IndexMode::Full
            } else {
                IndexMode::Incremental
            },
            request_key,
            config_fingerprint: indexer::DEFAULT_CONFIG_FINGERPRINT.to_owned(),
            changed_paths: None,
        },
        &CancellationContext::default(),
    )?;
    let progress = outcome.job.progress;
    Ok(IndexReport {
        schema_version: CURRENT_SCHEMA_VERSION,
        job_id: outcome.job.id.as_str().to_owned(),
        generation_id: outcome.generation_id.as_str().to_owned(),
        state: index_job_state(outcome.job.state),
        deduplicated: outcome.deduplicated,
        files_discovered: progress.files_discovered,
        files_reused: progress.files_reused,
        files_parsed: progress.files_parsed,
        files_failed: progress.files_failed,
        files_persisted: progress.files_persisted,
        files_deleted: progress.files_deleted,
        warning_count: progress.warning_count,
        warnings: outcome.warnings,
    })
}

fn status_report(root: &RepositoryRoot) -> Result<StatusReport, CliError> {
    let (identity, storage) = open_storage(root)?;
    let summary = storage.index_summary()?;
    Ok(map_status(identity, &storage, summary))
}

fn map_status(
    identity: RepositoryIdentity,
    storage: &Storage,
    summary: IndexSummary,
) -> StatusReport {
    StatusReport {
        schema_version: CURRENT_SCHEMA_VERSION,
        repository_root: identity.root().as_utf8().to_owned(),
        repository_id: identity.id().as_str().to_owned(),
        storage_role: storage.role().as_str(),
        active_generation_id: summary
            .active_generation_id
            .map(|id| id.as_str().to_owned()),
        active_files: summary.active_files,
        active_facts: summary.active_facts,
        active_symbols: summary.active_symbols,
        latest_job: summary.latest_job.map(|job| JobReport {
            id: job.id.as_str().to_owned(),
            generation_id: job.generation_id.map(|id| id.as_str().to_owned()),
            mode: job.mode,
            state: job.state.as_str(),
            cancel_requested: job.cancel_requested,
            files_discovered: job.progress.files_discovered,
            files_reused: job.progress.files_reused,
            files_parsed: job.progress.files_parsed,
            files_failed: job.progress.files_failed,
            files_persisted: job.progress.files_persisted,
            files_deleted: job.progress.files_deleted,
            warning_count: job.progress.warning_count,
            error_summary: job.error_summary,
        }),
    }
}

const fn index_job_state(state: ca_engine::indexing::IndexJobState) -> &'static str {
    use ca_engine::indexing::IndexJobState;
    match state {
        IndexJobState::Queued => "queued",
        IndexJobState::Scanning => "scanning",
        IndexJobState::Parsing => "parsing",
        IndexJobState::Resolving => "resolving",
        IndexJobState::Committing => "committing",
        IndexJobState::Completed => "completed",
        IndexJobState::Cancelled => "cancelled",
        IndexJobState::Failed => "failed",
        IndexJobState::Interrupted => "interrupted",
    }
}

fn print_json_or_text<T: Serialize>(
    value: &T,
    json: bool,
    text: impl FnOnce(),
) -> Result<(), CliError> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        text();
    }
    Ok(())
}

async fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Serve {
            root,
            watch,
            watch_debounce_ms,
            watch_reconcile_seconds,
            watch_poll,
            memory_write,
        } => {
            let root = resolve_root(Some(root))?;
            init_serve_tracing()?;
            tracing::info!(
                repository_root = root.as_utf8(),
                "starting MCP stdio service"
            );
            let backend = mcp_backend::McpBackend::with_watch(
                root,
                watcher::WatchConfig {
                    enabled: watch,
                    debounce: std::time::Duration::from_millis(watch_debounce_ms),
                    reconcile_interval: std::time::Duration::from_secs(watch_reconcile_seconds),
                    force_polling: watch_poll,
                    ..watcher::WatchConfig::default()
                },
                memory_write,
            )?;
            ca_mcp::serve_stdio(Arc::new(backend)).await?;
        }
        Command::Doctor { root, json } => {
            let root = resolve_root(root)?;
            let report = doctor_report(&root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("CodeAtlas {}", report.binary.version);
                println!("repository root: {}", report.configuration.repository_root);
                println!("repository id: {}", report.configuration.repository_id);
                println!("repository kind: {}", report.configuration.repository_kind);
                println!("data directory: {}", report.configuration.data_directory);
                println!(
                    "storage permissions: {}",
                    report.configuration.storage_permissions
                );
                println!(
                    "repository lifecycle: {}",
                    report.capabilities.repository_lifecycle
                );
                println!(
                    "database: {}; storage role: {}; parsers: {}; startup network: disabled",
                    report.capabilities.database,
                    report.capabilities.storage_role,
                    report.capabilities.parsers
                );
            }
        }
        Command::Index {
            root,
            full,
            request_key,
            json,
        } => {
            let report = run_index(resolve_root(Some(root))?, full, request_key)?;
            print_json_or_text(&report, json, || {
                println!("job: {} ({})", report.job_id, report.state);
                println!("generation: {}", report.generation_id);
                println!(
                    "files: discovered={}, reused={}, parsed={}, failed={}, persisted={}, deleted={}",
                    report.files_discovered,
                    report.files_reused,
                    report.files_parsed,
                    report.files_failed,
                    report.files_persisted,
                    report.files_deleted
                );
                println!("warnings: {}", report.warning_count);
            })?;
        }
        Command::Status { root, json } => {
            let report = status_report(&resolve_root(Some(root))?)?;
            print_json_or_text(&report, json, || {
                println!("repository: {}", report.repository_root);
                println!("storage role: {}", report.storage_role);
                println!(
                    "active generation: {}",
                    report.active_generation_id.as_deref().unwrap_or("none")
                );
                println!(
                    "active: files={}, facts={}, symbols={}",
                    report.active_files, report.active_facts, report.active_symbols
                );
                if let Some(job) = &report.latest_job {
                    println!("latest job: {} ({})", job.id, job.state);
                }
            })?;
        }
        Command::Integrate {
            target:
                IntegrationTarget::Codex {
                    root,
                    binary,
                    config,
                    remove,
                    dry_run: _,
                    apply,
                },
        } => {
            let result = codex_integration::run(codex_integration::Request {
                root,
                binary,
                config,
                remove,
                apply,
            })?;
            print!("{}", result.diff);
            if apply {
                if let Some(backup) = result.backup {
                    println!("backup: {}", backup.display());
                } else {
                    println!("backup: not needed (new config)");
                }
                println!("applied: {}", result.config.display());
            } else {
                println!("dry-run: no files changed");
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
