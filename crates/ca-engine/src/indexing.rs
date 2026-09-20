use std::{
    collections::{BTreeMap, HashSet},
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ca_core::{ByteRange, CancellationContext, GenerationId, JobId};
use thiserror::Error;

use crate::repository::{FileScanner, RelativeSourcePath, SourceFile, SourceReader};
use crate::resolution::{
    ManifestData, ResolutionGraph, ResolutionInput, ResolutionLimits, resolve_generation,
};

static NEXT_GENERATION_ID: AtomicU64 = AtomicU64::new(1);
const DURABLE_CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexMode {
    Incremental,
    Full,
}

impl IndexMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incremental => "incremental",
            Self::Full => "full",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexJobState {
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

impl IndexJobState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::Interrupted
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexProgress {
    pub files_discovered: u64,
    pub files_reused: u64,
    pub files_parsed: u64,
    pub files_failed: u64,
    pub files_persisted: u64,
    pub files_deleted: u64,
    pub warning_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexJob {
    pub id: JobId,
    pub generation_id: Option<GenerationId>,
    pub state: IndexJobState,
    pub cancel_requested: bool,
    pub progress: IndexProgress,
    pub error_summary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedJob {
    pub job: IndexJob,
    pub deduplicated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveFileVersion {
    pub relative_path: String,
    pub content_hash: String,
    pub grammar_fingerprint: String,
    pub query_fingerprint: String,
    pub extractor_fingerprint: String,
    pub config_fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractionFingerprints {
    pub grammar: String,
    pub query: String,
    pub extractor: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexedCategory {
    Symbol,
    Scope,
    Import,
    Reference,
    CallSite,
    Condition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedObservation {
    pub category: IndexedCategory,
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
    pub attributes: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub byte_range: Option<ByteRange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedFile {
    pub relative_path: String,
    pub content_hash: String,
    pub byte_length: u64,
    pub language: String,
    pub grammar_fingerprint: String,
    pub query_fingerprint: String,
    pub extractor_fingerprint: String,
    pub parse_status: String,
    pub coverage: String,
    pub observations: Vec<IndexedObservation>,
    pub diagnostics: Vec<IndexedDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexRequest {
    pub mode: IndexMode,
    pub request_key: Option<String>,
    pub config_fingerprint: String,
    pub changed_paths: Option<Vec<RelativeSourcePath>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexOutcome {
    pub job: IndexJob,
    pub generation_id: GenerationId,
    pub warnings: Vec<String>,
    pub deduplicated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexLimits {
    pub worker_count: usize,
    pub source_queue_capacity: usize,
    pub persist_batch_size: usize,
    pub max_warnings: usize,
}

impl Default for IndexLimits {
    fn default() -> Self {
        let workers = thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .clamp(1, 8);
        Self {
            worker_count: workers,
            source_queue_capacity: workers.saturating_mul(2),
            persist_batch_size: 16,
            max_warnings: 256,
        }
    }
}

impl IndexLimits {
    fn validate(self) -> Result<(), IndexingError> {
        if self.worker_count == 0
            || self.worker_count > 64
            || self.source_queue_capacity == 0
            || self.source_queue_capacity > 1_024
            || self.persist_batch_size == 0
            || self.persist_batch_size > 1_000
            || self.max_warnings == 0
            || self.max_warnings > 10_000
        {
            return Err(IndexingError::InvalidLimits);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum IndexingError {
    #[error("invalid indexing limits")]
    InvalidLimits,
    #[error("index store operation failed: {0}")]
    Store(String),
    #[error("extractor setup failed: {0}")]
    Extractor(String),
    #[error("repository scan was incomplete: {0}")]
    ScanIncomplete(String),
    #[error("index job {0} is already in progress")]
    AlreadyRunning(String),
    #[error("deduplicated index job {job} already ended in state {state}")]
    DeduplicatedTerminal { job: String, state: &'static str },
    #[error("indexing cancelled")]
    Cancelled,
    #[error("index worker queue disconnected")]
    WorkerDisconnected,
    #[error("index worker panicked")]
    WorkerPanicked,
    #[error("resolution failed: {0}")]
    Resolution(String),
    #[error("system clock cannot produce an index generation ID")]
    Clock,
}

pub trait IndexStore: Sync {
    type Error: Error + Send + Sync + 'static;

    fn create_job(
        &self,
        mode: IndexMode,
        request_key: Option<&str>,
    ) -> Result<CreatedJob, Self::Error>;
    fn update_job(
        &self,
        job_id: &JobId,
        state: IndexJobState,
        progress: &IndexProgress,
        error_summary: Option<&str>,
    ) -> Result<(), Self::Error>;
    fn job_status(&self, job_id: &JobId) -> Result<IndexJob, Self::Error>;
    fn active_files(&self) -> Result<Vec<ActiveFileVersion>, Self::Error>;
    fn begin_generation(
        &self,
        job_id: &JobId,
        generation_id: &GenerationId,
        config_fingerprint: &str,
        extractor_set_fingerprint: &str,
        reuse_parent: bool,
        deleted_paths: Vec<String>,
    ) -> Result<(), Self::Error>;
    fn reuse_files(
        &self,
        generation_id: &GenerationId,
        relative_paths: Vec<String>,
    ) -> Result<(), Self::Error>;
    fn stage_files(
        &self,
        generation_id: &GenerationId,
        files: Vec<IndexedFile>,
    ) -> Result<(), Self::Error>;
    fn resolution_input(
        &self,
        generation_id: &GenerationId,
    ) -> Result<ResolutionInput, Self::Error>;
    fn replace_resolution_graph(
        &self,
        generation_id: &GenerationId,
        graph: ResolutionGraph,
    ) -> Result<(), Self::Error>;
    fn complete_generation(
        &self,
        generation_id: &GenerationId,
        scan_complete: bool,
        coverage_status: &str,
        warning_count: u64,
        failure_summary: Option<&str>,
    ) -> Result<(), Self::Error>;
    fn activate_generation(&self, generation_id: &GenerationId) -> Result<(), Self::Error>;
    fn abandon_generation(
        &self,
        generation_id: Option<&GenerationId>,
        job_id: &JobId,
        state: IndexJobState,
        progress: &IndexProgress,
        error_summary: &str,
    ) -> Result<(), Self::Error>;
    fn gc_abandoned(&self) -> Result<(), Self::Error>;
}

pub trait ExtractionWorker: Send {
    fn extract(
        &mut self,
        source: &SourceFile,
        cancellation: &CancellationContext,
    ) -> Result<IndexedFile, ExtractionFailure>;
}

pub trait ExtractionWorkerFactory: Sync {
    type Worker: ExtractionWorker;

    fn create(&self) -> Result<Self::Worker, ExtractionFailure>;
    fn fingerprints(
        &self,
        path: &RelativeSourcePath,
    ) -> Result<ExtractionFingerprints, ExtractionFailure>;
    fn extractor_set_fingerprint(&self) -> String;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractionFailure {
    pub cancelled: bool,
    pub message: String,
}

struct SourceTask {
    source: SourceFile,
}

pub struct IndexService<'a, S, F> {
    scanner: FileScanner,
    reader: SourceReader,
    store: &'a S,
    extractor_factory: &'a F,
    limits: IndexLimits,
}

impl<'a, S, F> IndexService<'a, S, F>
where
    S: IndexStore,
    F: ExtractionWorkerFactory,
{
    pub fn new(
        reader: SourceReader,
        store: &'a S,
        extractor_factory: &'a F,
        limits: IndexLimits,
    ) -> Result<Self, IndexingError> {
        limits.validate()?;
        Ok(Self {
            scanner: FileScanner::new(reader.clone()),
            reader,
            store,
            extractor_factory,
            limits,
        })
    }

    pub fn run(
        &self,
        request: IndexRequest,
        cancellation: &CancellationContext,
    ) -> Result<IndexOutcome, IndexingError> {
        let created = self.prepare(&request)?;
        if created.deduplicated {
            if created.job.state == IndexJobState::Completed
                && let Some(generation_id) = created.job.generation_id.clone()
            {
                return Ok(IndexOutcome {
                    job: created.job,
                    generation_id,
                    warnings: Vec::new(),
                    deduplicated: true,
                });
            }
            if !created.job.state.is_terminal() {
                return Err(IndexingError::AlreadyRunning(
                    created.job.id.as_str().to_owned(),
                ));
            }
            return Err(IndexingError::DeduplicatedTerminal {
                job: created.job.id.as_str().to_owned(),
                state: job_state_name(created.job.state),
            });
        }
        self.run_prepared(created.job, request, cancellation)
    }

    /// Validate an index request and durably create or deduplicate its queued job.
    ///
    /// This performs no repository traversal or parsing, allowing protocol
    /// adapters to return the job ID before bounded background execution starts.
    pub fn prepare(&self, request: &IndexRequest) -> Result<CreatedJob, IndexingError> {
        if request.config_fingerprint.is_empty() || request.config_fingerprint.len() > 128 {
            return Err(IndexingError::InvalidLimits);
        }
        if let Some(paths) = &request.changed_paths {
            if paths.is_empty() || paths.len() > 100_000 {
                return Err(IndexingError::InvalidLimits);
            }
            let unique = paths.iter().collect::<HashSet<_>>();
            if unique.len() != paths.len() {
                return Err(IndexingError::InvalidLimits);
            }
        }
        self.store
            .create_job(request.mode, request.request_key.as_deref())
            .map_err(store_error)
    }

    /// Execute a previously prepared, non-deduplicated queued job.
    pub fn run_prepared(
        &self,
        job: IndexJob,
        request: IndexRequest,
        cancellation: &CancellationContext,
    ) -> Result<IndexOutcome, IndexingError> {
        let mut progress = IndexProgress::default();
        let mut generation_id = None;
        let result = self.run_pipeline(
            &job,
            &request,
            cancellation,
            &mut progress,
            &mut generation_id,
        );
        match result {
            Ok((generation, warnings)) => {
                let completed = self.store.job_status(&job.id).map_err(store_error)?;
                Ok(IndexOutcome {
                    job: completed,
                    generation_id: generation,
                    warnings,
                    deduplicated: false,
                })
            }
            Err(error) => {
                let state = if matches!(error, IndexingError::Cancelled) {
                    IndexJobState::Cancelled
                } else {
                    IndexJobState::Failed
                };
                let summary = error.to_string();
                self.store
                    .abandon_generation(generation_id.as_ref(), &job.id, state, &progress, &summary)
                    .map_err(store_error)?;
                let _ = self.store.gc_abandoned();
                Err(error)
            }
        }
    }

    fn run_pipeline(
        &self,
        job: &IndexJob,
        request: &IndexRequest,
        cancellation: &CancellationContext,
        progress: &mut IndexProgress,
        generation_out: &mut Option<GenerationId>,
    ) -> Result<(GenerationId, Vec<String>), IndexingError> {
        self.check_cancelled(&job.id, cancellation)?;
        self.store
            .update_job(&job.id, IndexJobState::Scanning, progress, None)
            .map_err(store_error)?;
        let active = self
            .store
            .active_files()
            .map_err(store_error)?
            .into_iter()
            .map(|file| (file.relative_path.clone(), file))
            .collect::<BTreeMap<_, _>>();
        let partial_paths = request.changed_paths.as_ref().filter(|paths| {
            request.mode == IndexMode::Incremental
                && !active.is_empty()
                && paths.iter().all(|path| active.contains_key(path.as_str()))
        });
        let scan = partial_paths.map_or_else(
            || self.scanner.scan_with_cancellation(cancellation),
            |paths| self.scanner.scan_paths(paths, cancellation),
        );
        progress.warning_count = u64::try_from(scan.diagnostics.len()).unwrap_or(u64::MAX);
        let mut warnings = scan
            .diagnostics
            .iter()
            .take(self.limits.max_warnings)
            .map(|diagnostic| format!("{}: {}", diagnostic.path, diagnostic.message))
            .collect::<Vec<_>>();
        self.check_cancelled(&job.id, cancellation)?;
        if !scan.complete {
            let summary = warnings
                .first()
                .cloned()
                .unwrap_or_else(|| "repository traversal did not complete".to_owned());
            return Err(IndexingError::ScanIncomplete(summary));
        }
        let discovered = scan
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<HashSet<_>>();
        let deleted_paths = partial_paths.map_or_else(
            || {
                active
                    .keys()
                    .filter(|path| !discovered.contains(path.as_str()))
                    .cloned()
                    .collect::<Vec<_>>()
            },
            |paths| {
                paths
                    .iter()
                    .filter(|path| !discovered.contains(path.as_str()))
                    .map(|path| path.as_str().to_owned())
                    .collect::<Vec<_>>()
            },
        );
        progress.files_deleted = u64::try_from(deleted_paths.len()).unwrap_or(u64::MAX);
        if let Some(paths) = partial_paths {
            let unchanged = active.len().saturating_sub(paths.len());
            let unchanged = u64::try_from(unchanged).unwrap_or(u64::MAX);
            progress.files_reused = unchanged;
            progress.files_persisted = unchanged;
            progress.files_discovered =
                u64::try_from(active.len().saturating_sub(deleted_paths.len())).unwrap_or(u64::MAX);
        } else {
            progress.files_discovered = u64::try_from(scan.files.len()).unwrap_or(u64::MAX);
        }

        self.store
            .update_job(&job.id, IndexJobState::Parsing, progress, None)
            .map_err(store_error)?;
        let generation = new_generation_id()?;
        *generation_out = Some(generation.clone());
        self.store
            .begin_generation(
                &job.id,
                &generation,
                &request.config_fingerprint,
                &self.extractor_factory.extractor_set_fingerprint(),
                request.mode == IndexMode::Incremental,
                deleted_paths,
            )
            .map_err(store_error)?;

        let parse_result = self.parse_and_stage(
            &job.id,
            &generation,
            request,
            scan.files,
            &active,
            cancellation,
            progress,
            &mut warnings,
        );
        parse_result?;

        self.store
            .update_job(&job.id, IndexJobState::Resolving, progress, None)
            .map_err(store_error)?;
        self.check_cancelled(&job.id, cancellation)?;
        let mut resolution_input = self
            .store
            .resolution_input(&generation)
            .map_err(store_error)?;
        match self.reader.read_supported_root_manifests() {
            Ok(manifests) => {
                resolution_input.manifests = manifests
                    .into_iter()
                    .map(|manifest| ManifestData {
                        relative_path: manifest.relative_path().as_str().to_owned(),
                        contents: manifest.text().to_owned(),
                    })
                    .collect();
            }
            Err(error) => {
                progress.warning_count = progress.warning_count.saturating_add(1);
                push_warning(
                    &mut warnings,
                    self.limits.max_warnings,
                    format!("supported manifest was not read: {error}"),
                );
            }
        }
        let graph =
            resolve_generation(&resolution_input, ResolutionLimits::default(), cancellation)
                .map_err(|error| {
                    if cancellation.is_cancelled() {
                        IndexingError::Cancelled
                    } else {
                        IndexingError::Resolution(error.to_string())
                    }
                })?;
        for diagnostic in &graph.diagnostics {
            progress.warning_count = progress.warning_count.saturating_add(1);
            push_warning(
                &mut warnings,
                self.limits.max_warnings,
                format!(
                    "{}: {}: {}",
                    diagnostic.relative_path, diagnostic.code, diagnostic.message
                ),
            );
        }
        self.store
            .replace_resolution_graph(&generation, graph)
            .map_err(store_error)?;
        self.check_cancelled(&job.id, cancellation)?;
        self.store
            .update_job(&job.id, IndexJobState::Committing, progress, None)
            .map_err(store_error)?;
        let coverage = if progress.files_failed == 0 && warnings.is_empty() {
            "syntax_and_name_resolution"
        } else {
            "ready_with_warnings"
        };
        self.store
            .complete_generation(&generation, true, coverage, progress.warning_count, None)
            .map_err(store_error)?;
        self.store
            .activate_generation(&generation)
            .map_err(store_error)?;
        self.store.gc_abandoned().map_err(store_error)?;
        Ok((generation, warnings))
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_and_stage(
        &self,
        job_id: &JobId,
        generation_id: &GenerationId,
        request: &IndexRequest,
        scanned_files: Vec<crate::repository::ScannedFile>,
        active: &BTreeMap<String, ActiveFileVersion>,
        cancellation: &CancellationContext,
        progress: &mut IndexProgress,
        warnings: &mut Vec<String>,
    ) -> Result<(), IndexingError> {
        let (source_tx, source_rx) =
            mpsc::sync_channel::<SourceTask>(self.limits.source_queue_capacity);
        let source_rx = Arc::new(Mutex::new(source_rx));
        let (result_tx, result_rx) = mpsc::channel::<Result<IndexedFile, ExtractionFailure>>();
        let shared_cancel = cancellation.clone();
        let mut sent = 0_usize;
        let mut last_durable_cancel_check = Instant::now();

        thread::scope(|scope| -> Result<(), IndexingError> {
            let mut handles = Vec::with_capacity(self.limits.worker_count);
            for _ in 0..self.limits.worker_count {
                let receiver = Arc::clone(&source_rx);
                let sender = result_tx.clone();
                let worker_cancel = shared_cancel.clone();
                let factory = self.extractor_factory;
                handles.push(scope.spawn(move || {
                    let mut worker = match factory.create() {
                        Ok(worker) => worker,
                        Err(error) => {
                            let _ = sender.send(Err(error));
                            return;
                        }
                    };
                    loop {
                        let task = match receiver.lock() {
                            Ok(guard) => guard.recv(),
                            Err(_) => return,
                        };
                        let Ok(task) = task else {
                            return;
                        };
                        let result = worker.extract(&task.source, &worker_cancel);
                        if sender.send(result).is_err() {
                            return;
                        }
                    }
                }));
            }
            drop(result_tx);

            for scanned in scanned_files {
                if shared_cancel.is_cancelled() {
                    return Err(IndexingError::Cancelled);
                }
                if last_durable_cancel_check.elapsed() >= DURABLE_CANCELLATION_POLL_INTERVAL {
                    self.check_cancelled(job_id, &shared_cancel)?;
                    last_durable_cancel_check = Instant::now();
                }
                let fingerprints = self
                    .extractor_factory
                    .fingerprints(&scanned.relative_path)
                    .map_err(|error| IndexingError::Extractor(error.message))?;
                let reusable = request.mode == IndexMode::Incremental
                    && active
                        .get(scanned.relative_path.as_str())
                        .is_some_and(|prior| {
                            prior.content_hash == scanned.content_hash
                                && prior.grammar_fingerprint == fingerprints.grammar
                                && prior.query_fingerprint == fingerprints.query
                                && prior.extractor_fingerprint == fingerprints.extractor
                                && prior.config_fingerprint == request.config_fingerprint
                        });
                if reusable {
                    progress.files_reused = progress.files_reused.saturating_add(1);
                    progress.files_persisted = progress.files_persisted.saturating_add(1);
                    continue;
                }
                let source = self
                    .reader
                    .read(&scanned.relative_path)
                    .map_err(|error| IndexingError::ScanIncomplete(error.to_string()))?;
                if source.content_hash() != scanned.content_hash {
                    push_warning(
                        warnings,
                        self.limits.max_warnings,
                        format!(
                            "{} changed between traversal and parsing; indexed the revalidated bytes",
                            scanned.relative_path.as_str()
                        ),
                    );
                    progress.warning_count = progress.warning_count.saturating_add(1);
                }
                send_bounded(&source_tx, SourceTask { source }, &shared_cancel, || {
                    self.store
                        .job_status(job_id)
                        .map(|job| job.cancel_requested)
                })
                .map_err(|error| match error {
                    SendFailure::Cancelled => IndexingError::Cancelled,
                    SendFailure::Disconnected => IndexingError::WorkerDisconnected,
                    SendFailure::Store(message) => IndexingError::Store(message),
                })?;
                sent += 1;
            }
            self.check_cancelled(job_id, &shared_cancel)?;
            drop(source_tx);

            let mut persist_batch = Vec::with_capacity(self.limits.persist_batch_size);
            for _ in 0..sent {
                let result = loop {
                    self.check_cancelled(job_id, &shared_cancel)?;
                    match result_rx.recv_timeout(Duration::from_millis(10)) {
                        Ok(result) => break result,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return Err(IndexingError::WorkerDisconnected);
                        }
                    }
                };
                match result {
                    Ok(file) => {
                        progress.files_parsed = progress.files_parsed.saturating_add(1);
                        let diagnostic_warnings = file
                            .diagnostics
                            .iter()
                            .filter(|diagnostic| {
                                matches!(diagnostic.severity.as_str(), "warning" | "error")
                            })
                            .count();
                        progress.warning_count = progress
                            .warning_count
                            .saturating_add(u64::try_from(diagnostic_warnings).unwrap_or(u64::MAX));
                        if file.parse_status == "failed" {
                            progress.files_failed = progress.files_failed.saturating_add(1);
                            push_warning(
                                warnings,
                                self.limits.max_warnings,
                                format!("{}: parser coverage failed", file.relative_path),
                            );
                        } else if file.parse_status == "partial" {
                            push_warning(
                                warnings,
                                self.limits.max_warnings,
                                format!("{}: parser coverage is partial", file.relative_path),
                            );
                        }
                        persist_batch.push(file);
                        if persist_batch.len() >= self.limits.persist_batch_size {
                            let count = u64::try_from(persist_batch.len()).unwrap_or(u64::MAX);
                            self.store
                                .stage_files(generation_id, std::mem::take(&mut persist_batch))
                                .map_err(store_error)?;
                            progress.files_persisted =
                                progress.files_persisted.saturating_add(count);
                        }
                    }
                    Err(error) if error.cancelled => {
                        shared_cancel.cancel();
                        return Err(IndexingError::Cancelled);
                    }
                    Err(error) => return Err(IndexingError::Extractor(error.message)),
                }
                self.store
                    .update_job(job_id, IndexJobState::Parsing, progress, None)
                    .map_err(store_error)?;
            }
            if !persist_batch.is_empty() {
                let count = u64::try_from(persist_batch.len()).unwrap_or(u64::MAX);
                self.store
                    .stage_files(generation_id, persist_batch)
                    .map_err(store_error)?;
                progress.files_persisted = progress.files_persisted.saturating_add(count);
            }
            for handle in handles {
                handle.join().map_err(|_| IndexingError::WorkerPanicked)?;
            }
            Ok(())
        })
    }

    fn check_cancelled(
        &self,
        job_id: &JobId,
        cancellation: &CancellationContext,
    ) -> Result<(), IndexingError> {
        if cancellation.is_cancelled() {
            return Err(IndexingError::Cancelled);
        }
        let job = self.store.job_status(job_id).map_err(store_error)?;
        if job.cancel_requested {
            cancellation.cancel();
            return Err(IndexingError::Cancelled);
        }
        Ok(())
    }
}

fn store_error<E: Error>(error: E) -> IndexingError {
    IndexingError::Store(error.to_string())
}

const fn job_state_name(state: IndexJobState) -> &'static str {
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

fn new_generation_id() -> Result<GenerationId, IndexingError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| IndexingError::Clock)?
        .as_nanos();
    let sequence = NEXT_GENERATION_ID.fetch_add(1, Ordering::Relaxed);
    GenerationId::new(format!("g-{now:032x}-{sequence:016x}")).map_err(|_| IndexingError::Clock)
}

fn push_warning(warnings: &mut Vec<String>, limit: usize, warning: String) {
    if warnings.len() < limit {
        warnings.push(warning);
    }
}

enum SendFailure {
    Cancelled,
    Disconnected,
    Store(String),
}

fn send_bounded<T, E>(
    sender: &SyncSender<T>,
    mut value: T,
    cancellation: &CancellationContext,
    mut durable_cancelled: impl FnMut() -> Result<bool, E>,
) -> Result<(), SendFailure>
where
    E: Error,
{
    loop {
        if cancellation.is_cancelled() {
            return Err(SendFailure::Cancelled);
        }
        if durable_cancelled().map_err(|error| SendFailure::Store(error.to_string()))? {
            cancellation.cancel();
            return Err(SendFailure::Cancelled);
        }
        match sender.try_send(value) {
            Ok(()) => return Ok(()),
            Err(TrySendError::Full(returned)) => {
                value = returned;
                thread::yield_now();
            }
            Err(TrySendError::Disconnected(_)) => return Err(SendFailure::Disconnected),
        }
    }
}
