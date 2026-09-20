use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use ca_core::{CancellationContext, GenerationId, JobId, RepositoryRoot};
use ca_engine::{
    indexing::{
        IndexJob, IndexJobState, IndexLimits, IndexMode, IndexProgress, IndexRequest, IndexService,
        IndexingError,
    },
    memory::{
        MemoryDraft, MemoryError, MemoryEvidence, MemoryKind, MemoryLimits, MemoryRecord,
        MemorySearch, MemoryService,
    },
    repository::{
        RelativeSourcePath, RepositoryError, RepositoryIdentity, ScanPolicy, SourceReader,
    },
    retrieval::{
        Budgeted, ContextRequest, ImpactRequest, OutlineRequest, ReadCodeRequest as EngineReadCode,
        ReferencesRequest, RelationDirection, RepoMapRequest, RetrievalError, RetrievalLimits,
        RetrievalService, SearchFilter, SearchRequest, TraceRequest,
    },
};
use ca_languages::LanguageRegistry;
use ca_mcp::{
    AnalyzeImpactRequest, ApplicationEnvelope, ApplicationError, BackendRequest,
    BackendResourceRequest, CancelJobRequest, DirectionInput, FindReferencesRequest,
    ForgetMemoryRequest, GetFileOutlineRequest, GetRepoMapRequest, GetSymbolRequest,
    IndexModeInput, IndexRepositoryRequest, JobStatusRequest, MemoryKindInput, ReadCodeRequest,
    SearchMemoriesRequest, SearchSymbolsRequest, ToolBackend, TraceCallsRequest,
    UpsertMemoryRequest,
};
use ca_storage::{
    CancelJobResult, IndexSummary, JobRecord, JobState, Storage, StorageError, StoragePaths,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    indexer::{DEFAULT_CONFIG_FINGERPRINT, ParserFactory, StorageIndexAdapter},
    memory::StorageMemoryAdapter,
    retrieval::StorageRetrievalAdapter,
    watcher::{ReconcileTrigger, WatchConfig, WatchError, WatchHandle, WatchSnapshot},
};

const ENGINE_RESPONSE_BYTES: usize = 60 * 1_024;
const RESOURCE_RESPONSE_BYTES: usize = 28 * 1_024;
const MEMORY_RESPONSE_BYTES: usize = 28 * 1_024;

#[derive(Clone, Debug)]
struct CachedState {
    lifecycle: &'static str,
    repository_id: Option<String>,
    generation_id: Option<String>,
    storage_role: Option<&'static str>,
    active_files: u64,
    active_facts: u64,
    active_symbols: u64,
    latest_job: Option<Value>,
    coverage: Value,
}

impl Default for CachedState {
    fn default() -> Self {
        Self {
            lifecycle: "not_opened",
            repository_id: None,
            generation_id: None,
            storage_role: None,
            active_files: 0,
            active_facts: 0,
            active_symbols: 0,
            latest_job: None,
            coverage: json!({}),
        }
    }
}

#[derive(Default)]
struct RuntimeState {
    cached: Mutex<CachedState>,
    active_jobs: Mutex<BTreeMap<String, ActiveJob>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    index_gate: Mutex<()>,
    watcher: Mutex<Option<WatchHandle>>,
    owner_storage: Mutex<Option<(RepositoryIdentity, Arc<Storage>)>>,
}

#[derive(Clone)]
struct ActiveJob {
    cancellation: CancellationContext,
    storage: Arc<Storage>,
    request_key: Option<String>,
    job: IndexJob,
    repository_id: String,
    origin: ActiveJobOrigin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveJobOrigin {
    Manual,
    Watch,
}

fn manual_index_is_blocked(origins: impl IntoIterator<Item = ActiveJobOrigin>) -> bool {
    origins
        .into_iter()
        .any(|origin| origin == ActiveJobOrigin::Manual)
}

pub struct McpBackend {
    root: RepositoryRoot,
    runtime: Arc<RuntimeState>,
    watch_config: WatchConfig,
    memory_writes_enabled: bool,
}

impl McpBackend {
    pub(crate) fn with_watch(
        root: RepositoryRoot,
        watch_config: WatchConfig,
        memory_writes_enabled: bool,
    ) -> Result<Self, WatchError> {
        Ok(Self {
            root,
            runtime: Arc::new(RuntimeState::default()),
            watch_config: watch_config.validate()?,
            memory_writes_enabled,
        })
    }

    fn repository_status(&self) -> Result<ApplicationEnvelope, ApplicationError> {
        let cached = self
            .runtime
            .cached
            .lock()
            .map_err(|_| internal_error("repository status cache is poisoned"))?
            .clone();
        let mut envelope = ApplicationEnvelope::success(
            cached.repository_id.clone(),
            cached.generation_id.clone(),
            cached.lifecycle,
            json!({
                "root": self.root.as_utf8(),
                "lifecycle": cached.lifecycle,
                "storage_role": cached.storage_role,
                "active_files": cached.active_files,
                "active_facts": cached.active_facts,
                "active_symbols": cached.active_symbols,
                "latest_job": cached.latest_job,
                "languages": ["dart", "csharp", "rust", "go", "java", "javascript", "jsx", "typescript", "tsx"],
                "protocol_versions": ["2026-07-28", "2025-11-25"],
                "codeatlas_version": env!("CARGO_PKG_VERSION"),
                "storage_schema_version": ca_storage::CURRENT_SCHEMA_VERSION,
                "runtime_network": false,
                "auto_index": false,
                "watch": watch_value(&self.watch_snapshot()),
                "memory": {
                    "enabled": true,
                    "writes_enabled": self.memory_writes_enabled,
                    "source": "explicit_project_notes",
                    "authoritative_code_facts": false
                }
            }),
        );
        envelope.coverage = cached.coverage;
        Ok(envelope)
    }

    fn watch_snapshot(&self) -> WatchSnapshot {
        self.runtime
            .watcher
            .lock()
            .ok()
            .and_then(|watcher| watcher.as_ref().map(WatchHandle::snapshot))
            .unwrap_or_else(|| WatchSnapshot::configured(self.watch_config))
    }

    fn ensure_watcher(
        &self,
        identity: &RepositoryIdentity,
        storage: Arc<Storage>,
    ) -> Result<(), ApplicationError> {
        if !self.watch_config.enabled {
            return Ok(());
        }
        let mut watcher = self
            .runtime
            .watcher
            .lock()
            .map_err(|_| internal_error("watcher registry is poisoned"))?;
        if watcher.is_some() {
            return Ok(());
        }
        let runtime = Arc::clone(&self.runtime);
        let worker_identity = identity.clone();
        let worker_storage = Arc::clone(&storage);
        let handle = WatchHandle::start(
            self.root.as_path().to_path_buf(),
            self.watch_config,
            move |trigger, paths, cancellation| {
                run_watch_reconciliation(
                    &runtime,
                    &worker_identity,
                    &worker_storage,
                    trigger,
                    paths,
                    cancellation,
                )
                .map_err(|error| error.to_string())
            },
        )
        .map_err(watch_error)?;
        *watcher = Some(handle);
        Ok(())
    }

    fn reap_finished_workers(&self) -> Result<(), ApplicationError> {
        let mut workers = self
            .runtime
            .workers
            .lock()
            .map_err(|_| internal_error("index worker registry is poisoned"))?;
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                workers
                    .swap_remove(index)
                    .join()
                    .map_err(|_| internal_error("index worker panicked"))?;
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    fn prune_terminal_active_jobs(&self) -> Result<(), ApplicationError> {
        let active = self
            .runtime
            .active_jobs
            .lock()
            .map_err(|_| internal_error("active job registry is poisoned"))?
            .iter()
            .map(|(key, job)| (key.clone(), Arc::clone(&job.storage), job.job.id.clone()))
            .collect::<Vec<_>>();
        let mut terminal = BTreeSet::new();
        for (key, storage, job_id) in active {
            if storage
                .job_status(&job_id)
                .map_err(storage_error)?
                .state
                .is_terminal()
            {
                terminal.insert(key);
            }
        }
        if terminal.is_empty() {
            return Ok(());
        }
        self.runtime
            .active_jobs
            .lock()
            .map_err(|_| internal_error("active job registry is poisoned"))?
            .retain(|key, _| !terminal.contains(key));
        Ok(())
    }

    fn open_storage(&self) -> Result<(RepositoryIdentity, Arc<Storage>), ApplicationError> {
        if let Some((identity, storage)) = self
            .runtime
            .owner_storage
            .lock()
            .map_err(|_| internal_error("owner storage registry is poisoned"))?
            .as_ref()
        {
            if storage.role() == ca_storage::StorageRole::Owner {
                self.ensure_watcher(identity, Arc::clone(storage))?;
            }
            return Ok((identity.clone(), Arc::clone(storage)));
        }
        let identity = RepositoryIdentity::derive(self.root.clone()).map_err(repository_error)?;
        let paths = StoragePaths::platform(identity.id(), identity.root().as_path())
            .map_err(storage_error)?;
        let storage = Arc::new(Storage::open(identity.id().clone(), paths).map_err(storage_error)?);
        self.refresh_cache(&identity, storage.as_ref())?;
        if storage.role() == ca_storage::StorageRole::Owner {
            *self
                .runtime
                .owner_storage
                .lock()
                .map_err(|_| internal_error("owner storage registry is poisoned"))? =
                Some((identity.clone(), Arc::clone(&storage)));
            self.ensure_watcher(&identity, Arc::clone(&storage))?;
        }
        Ok((identity, storage))
    }

    fn refresh_cache(
        &self,
        identity: &RepositoryIdentity,
        storage: &Storage,
    ) -> Result<(), ApplicationError> {
        let summary = storage.index_summary().map_err(storage_error)?;
        refresh_cache(&self.runtime, identity, storage, summary)
    }

    fn index_repository(
        &self,
        request: IndexRepositoryRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        self.reap_finished_workers()?;
        self.prune_terminal_active_jobs()?;
        if let Some(request_key) = request.request_key.as_deref() {
            let active = self
                .runtime
                .active_jobs
                .lock()
                .map_err(|_| internal_error("active job registry is poisoned"))?;
            if let Some(active) = active
                .values()
                .find(|active| active.request_key.as_deref() == Some(request_key))
            {
                let job = active
                    .storage
                    .job_status(&active.job.id)
                    .map_err(storage_error)?;
                return Ok(stored_job_envelope(
                    &active.repository_id,
                    &job,
                    "deduplicated",
                ));
            }
        }
        let manual_job_active = {
            let active = self
                .runtime
                .active_jobs
                .lock()
                .map_err(|_| internal_error("active job registry is poisoned"))?;
            manual_index_is_blocked(active.values().map(|job| job.origin))
        };
        if manual_job_active {
            return Err(ApplicationError::new(
                "WRITER_BUSY",
                "another repository index job is active; retry later",
            )
            .retryable(Some(250)));
        }
        let (identity, storage) = self.open_storage()?;
        if storage.role() != ca_storage::StorageRole::Owner {
            return Err(ApplicationError::new(
                "WRITER_BUSY",
                "repository index writer is owned by another process; retry later",
            )
            .retryable(Some(250)));
        }
        *self
            .runtime
            .owner_storage
            .lock()
            .map_err(|_| internal_error("owner storage registry is poisoned"))? =
            Some((identity.clone(), Arc::clone(&storage)));
        self.ensure_watcher(&identity, Arc::clone(&storage))?;
        let index_request = IndexRequest {
            mode: match request.mode {
                IndexModeInput::Incremental => IndexMode::Incremental,
                IndexModeInput::Full => IndexMode::Full,
            },
            request_key: request.request_key,
            config_fingerprint: DEFAULT_CONFIG_FINGERPRINT.to_owned(),
            changed_paths: None,
        };
        let factory = ParserFactory;
        let adapter = StorageIndexAdapter::new(storage.as_ref());
        let service = IndexService::new(
            SourceReader::new(identity.clone(), ScanPolicy::default()),
            &adapter,
            &factory,
            IndexLimits::default(),
        )
        .map_err(indexing_error)?;
        let created = service.prepare(&index_request).map_err(indexing_error)?;
        let job = created.job.clone();
        if created.deduplicated {
            return Ok(job_envelope(
                identity.id().as_str(),
                &job,
                true,
                "deduplicated",
            ));
        }

        let cancellation = CancellationContext::default();
        let active_request_key = index_request.request_key.clone();
        self.runtime
            .active_jobs
            .lock()
            .map_err(|_| internal_error("active job registry is poisoned"))?
            .insert(
                job.id.as_str().to_owned(),
                ActiveJob {
                    cancellation: cancellation.clone(),
                    storage: Arc::clone(&storage),
                    request_key: active_request_key,
                    job: job.clone(),
                    repository_id: identity.id().as_str().to_owned(),
                    origin: ActiveJobOrigin::Manual,
                },
            );

        let worker_runtime = Arc::clone(&self.runtime);
        let worker_storage = Arc::clone(&storage);
        let worker_identity = identity.clone();
        let worker_job = job.clone();
        let job_key = job.id.as_str().to_owned();
        let spawn = thread::Builder::new()
            .name(format!("codeatlas-index-{}", job.id.as_str()))
            .spawn(move || {
                let result = match worker_runtime.index_gate.lock() {
                    Ok(_gate) => {
                        let adapter = StorageIndexAdapter::new(worker_storage.as_ref());
                        let factory = ParserFactory;
                        IndexService::new(
                            SourceReader::new(worker_identity.clone(), ScanPolicy::default()),
                            &adapter,
                            &factory,
                            IndexLimits::default(),
                        )
                        .and_then(|service| {
                            service.run_prepared(worker_job, index_request, &cancellation)
                        })
                    }
                    Err(_) => Err(IndexingError::Store(
                        "index serialization gate is poisoned".to_owned(),
                    )),
                };
                if let Err(error) = &result {
                    tracing::error!(job_id = job_key, error = %error, "index job ended with an application error");
                }
                if let Ok(summary) = worker_storage.index_summary() {
                    let _ = refresh_cache(
                        &worker_runtime,
                        &worker_identity,
                        worker_storage.as_ref(),
                        summary,
                    );
                }
                if let Ok(mut active_jobs) = worker_runtime.active_jobs.lock() {
                    active_jobs.remove(&job_key);
                }
            });

        let handle = match spawn {
            Ok(handle) => handle,
            Err(error) => {
                self.runtime
                    .active_jobs
                    .lock()
                    .map_err(|_| internal_error("active job registry is poisoned"))?
                    .remove(job.id.as_str());
                storage
                    .abandon_generation(
                        None,
                        job.id.clone(),
                        JobState::Failed,
                        ca_storage::JobProgress::default(),
                        format!("cannot spawn index worker: {error}"),
                    )
                    .map_err(storage_error)?;
                return Err(internal_error(format!(
                    "cannot spawn bounded index worker: {error}"
                )));
            }
        };
        self.runtime
            .workers
            .lock()
            .map_err(|_| internal_error("index worker registry is poisoned"))?
            .push(handle);
        Ok(job_envelope(identity.id().as_str(), &job, false, "queued"))
    }

    fn job_status(
        &self,
        request: JobStatusRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        let job_id = JobId::new(request.job_id).map_err(core_error)?;
        let (identity, storage) = self.open_storage()?;
        let job = storage.job_status(&job_id).map_err(storage_error)?;
        self.refresh_cache(&identity, &storage)?;
        Ok(stored_job_envelope(identity.id().as_str(), &job, "ok"))
    }

    fn cancel_job(
        &self,
        request: CancelJobRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        let job_id = JobId::new(request.job_id).map_err(core_error)?;
        let active = self
            .runtime
            .active_jobs
            .lock()
            .map_err(|_| internal_error("active job registry is poisoned"))?
            .get(job_id.as_str())
            .cloned();
        let identity = RepositoryIdentity::derive(self.root.clone()).map_err(repository_error)?;
        let storage = if let Some(active) = &active {
            Arc::clone(&active.storage)
        } else {
            self.open_storage()?.1
        };
        let result = storage
            .request_cancel(job_id.clone())
            .map_err(storage_error)?;
        if let Some(active) = &active {
            active.cancellation.cancel();
        }
        let job = storage.job_status(&job_id).map_err(storage_error)?;
        self.refresh_cache(&identity, storage.as_ref())?;
        let status = match result {
            CancelJobResult::Requested => "cancel_requested",
            CancelJobResult::AlreadyRequested => "already_requested",
            CancelJobResult::AlreadyCompleted => "already_completed",
        };
        Ok(stored_job_envelope(identity.id().as_str(), &job, status))
    }

    fn retrieval_service(
        &self,
    ) -> Result<(RepositoryIdentity, Arc<Storage>, ca_storage::ReadSnapshot), ApplicationError>
    {
        let (identity, storage) = self.open_storage()?;
        let snapshot = storage.read_snapshot().map_err(storage_error)?;
        Ok((identity, storage, snapshot))
    }

    fn search_symbols(
        &self,
        request: SearchSymbolsRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        validate_range("limit", request.limit, 1, 200)?;
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let service = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?;
        let result = service
            .search_symbols(SearchRequest {
                query: request.query,
                filter: SearchFilter {
                    language: request.language.map(|value| value.as_str().to_owned()),
                    kind: request.kind,
                    path_prefix: request.path_prefix,
                },
                limit: request.limit,
                cursor: request.cursor,
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn get_symbol(
        &self,
        request: GetSymbolRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        let max_bytes = engine_budget(request.max_bytes)?;
        let requested_generation = request
            .generation_id
            .map(GenerationId::new)
            .transpose()
            .map_err(core_error)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        if let Some(requested) = requested_generation
            && &requested != snapshot.generation_id()
        {
            return Err(ApplicationError::new(
                "GENERATION_MISMATCH",
                "requested generation is not the active snapshot for this repository",
            ));
        }
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .get_symbol(&request.symbol_id, max_bytes)
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn find_references(
        &self,
        request: FindReferencesRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        validate_range("limit", request.limit, 1, 200)?;
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .find_references(ReferencesRequest {
                symbol_id: request.symbol_id,
                include_candidates: request.include_candidates,
                limit: request.limit,
                cursor: request.cursor,
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn trace_calls(
        &self,
        request: TraceCallsRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        validate_graph_limits(
            request.depth,
            request.max_nodes,
            request.max_edges,
            request.deadline_ms,
        )?;
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .trace_calls(TraceRequest {
                symbol_id: request.symbol_id,
                direction: direction(request.direction),
                depth: request.depth,
                include_candidates: request.include_candidates,
                max_nodes: request.max_nodes,
                max_edges: request.max_edges,
                deadline: Duration::from_millis(request.deadline_ms),
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn get_file_outline(
        &self,
        request: GetFileOutlineRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        validate_range("limit", request.limit, 1, 2_000)?;
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .file_outline(OutlineRequest {
                relative_path: request.relative_path,
                limit: request.limit,
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn read_code(&self, request: ReadCodeRequest) -> Result<ApplicationEnvelope, ApplicationError> {
        if request.start_line == 0
            || request.end_line < request.start_line
            || request.end_line.saturating_sub(request.start_line) >= 400
        {
            return Err(ApplicationError::new(
                "INVALID_ARGUMENT",
                "line range must be ordered, one-based and contain at most 400 lines",
            ));
        }
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .read_code(EngineReadCode {
                relative_path: request.relative_path,
                start_line: request.start_line,
                end_line: request.end_line,
                expected_hash: request.expected_hash,
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn get_repo_map(
        &self,
        request: GetRepoMapRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        validate_range("depth", request.depth, 0, 8)?;
        validate_range("limit", request.limit, 1, 5_000)?;
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .repo_map(RepoMapRequest {
                scope: request.scope,
                depth: request.depth,
                limit: request.limit,
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn analyze_impact(
        &self,
        request: AnalyzeImpactRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        validate_graph_limits(
            request.depth,
            request.max_nodes,
            request.max_edges,
            request.deadline_ms,
        )?;
        if request.symbol_ids.is_empty() && request.changed_paths.is_empty() {
            return Err(ApplicationError::new(
                "INVALID_ARGUMENT",
                "at least one symbol_id or changed_path is required",
            ));
        }
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .analyze_impact(ImpactRequest {
                symbol_ids: request.symbol_ids,
                changed_paths: request.changed_paths,
                depth: request.depth,
                include_candidates: request.include_candidates,
                max_nodes: request.max_nodes,
                max_edges: request.max_edges,
                deadline: Duration::from_millis(request.deadline_ms),
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn build_context(
        &self,
        request: ca_mcp::BuildContextRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        let max_bytes = engine_budget(request.max_bytes)?;
        let (identity, storage, snapshot) = self.retrieval_service()?;
        let reader = SourceReader::new(identity, ScanPolicy::default());
        let adapter = StorageRetrievalAdapter::new(&snapshot);
        let result = RetrievalService::new(&adapter, &reader, RetrievalLimits::default())
            .map_err(retrieval_error)?
            .build_context(ContextRequest {
                query: request.query,
                scope: request.scope,
                include_candidates: request.include_candidates,
                max_bytes,
            })
            .map_err(retrieval_error)?;
        self.refresh_cache_from_snapshot(&storage, &snapshot)?;
        envelope_from_budgeted(result)
    }

    fn search_memories(
        &self,
        request: SearchMemoriesRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        validate_range("limit", request.limit, 1, 100)?;
        let (identity, storage) = self.open_storage()?;
        let adapter = StorageMemoryAdapter::new(&storage);
        let records = MemoryService::new(&adapter, MemoryLimits::default())
            .map_err(memory_error)?
            .search(MemorySearch {
                query: request.query,
                scope: request.scope,
                include_stale: request.include_stale,
                limit: request.limit,
            })
            .map_err(memory_error)?;
        let summary = storage.index_summary().map_err(storage_error)?;
        let generation_id = summary
            .active_generation_id
            .as_ref()
            .map(|value| value.as_str().to_owned());
        self.refresh_cache(&identity, &storage)?;
        bounded_memory_search_envelope(
            identity.id().as_str(),
            generation_id,
            !request.include_stale,
            records.iter().map(memory_value).collect(),
        )
    }

    fn upsert_memory(
        &self,
        request: UpsertMemoryRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        self.require_memory_writes()?;
        let (identity, storage) = self.open_storage()?;
        let adapter = StorageMemoryAdapter::new(&storage);
        let record = MemoryService::new(&adapter, MemoryLimits::default())
            .map_err(memory_error)?
            .upsert(MemoryDraft {
                memory_id: request.memory_id,
                text: request.text,
                kind: memory_kind(request.kind),
                author: request.author,
                origin: request.origin,
                scope: request.scope,
                evidence: request
                    .evidence
                    .into_iter()
                    .map(|item| MemoryEvidence {
                        relative_path: item.relative_path,
                        content_hash: item.content_hash,
                        symbol_id: item.symbol_id,
                    })
                    .collect(),
                expected_revision: request.expected_revision,
            })
            .map_err(memory_error)?;
        let summary = storage.index_summary().map_err(storage_error)?;
        self.refresh_cache(&identity, &storage)?;
        Ok(memory_envelope(
            identity.id().as_str(),
            summary
                .active_generation_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            "stored",
            json!({"memory": memory_value(&record)}),
        ))
    }

    fn forget_memory(
        &self,
        request: ForgetMemoryRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        self.require_memory_writes()?;
        let (identity, storage) = self.open_storage()?;
        let adapter = StorageMemoryAdapter::new(&storage);
        let record = MemoryService::new(&adapter, MemoryLimits::default())
            .map_err(memory_error)?
            .forget(request.memory_id, request.expected_revision)
            .map_err(memory_error)?;
        let summary = storage.index_summary().map_err(storage_error)?;
        self.refresh_cache(&identity, &storage)?;
        Ok(memory_envelope(
            identity.id().as_str(),
            summary
                .active_generation_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            "forgotten",
            json!({
                "memory_id": record.id,
                "deleted_revision": record.revision,
                "destructive": true
            }),
        ))
    }

    fn memory_resource(&self, id: &str) -> Result<ApplicationEnvelope, ApplicationError> {
        let (identity, storage) = self.open_storage()?;
        let adapter = StorageMemoryAdapter::new(&storage);
        let record = MemoryService::new(&adapter, MemoryLimits::default())
            .map_err(memory_error)?
            .get(id)
            .map_err(memory_error)?
            .ok_or_else(|| {
                ApplicationError::new("MEMORY_NOT_FOUND", format!("memory '{id}' does not exist"))
            })?;
        let summary = storage.index_summary().map_err(storage_error)?;
        self.refresh_cache(&identity, &storage)?;
        Ok(memory_envelope(
            identity.id().as_str(),
            summary
                .active_generation_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            "ok",
            json!({"memory": memory_value(&record)}),
        ))
    }

    fn require_memory_writes(&self) -> Result<(), ApplicationError> {
        if self.memory_writes_enabled {
            Ok(())
        } else {
            Err(ApplicationError::new(
                "MEMORY_WRITE_DISABLED",
                "memory writes are disabled; restart with trusted --memory-write policy to enable them",
            ))
        }
    }

    fn refresh_cache_from_snapshot(
        &self,
        storage: &Storage,
        snapshot: &ca_storage::ReadSnapshot,
    ) -> Result<(), ApplicationError> {
        let mut cached = self
            .runtime
            .cached
            .lock()
            .map_err(|_| internal_error("repository status cache is poisoned"))?;
        cached.lifecycle = if storage.role() == ca_storage::StorageRole::Owner {
            "ready"
        } else {
            "read_only_follower"
        };
        cached.repository_id = Some(snapshot.repository_id().as_str().to_owned());
        cached.generation_id = Some(snapshot.generation_id().as_str().to_owned());
        cached.storage_role = Some(storage.role().as_str());
        cached.coverage = coverage_value(snapshot.retrieval_coverage().map_err(storage_error)?);
        Ok(())
    }
}

fn run_watch_reconciliation(
    runtime: &Arc<RuntimeState>,
    identity: &RepositoryIdentity,
    storage: &Arc<Storage>,
    trigger: ReconcileTrigger,
    paths: Vec<PathBuf>,
    watcher_cancellation: &CancellationContext,
) -> Result<(), ApplicationError> {
    let _gate = runtime
        .index_gate
        .lock()
        .map_err(|_| internal_error("index serialization gate is poisoned"))?;
    if watcher_cancellation.is_cancelled() {
        return Ok(());
    }

    let request = IndexRequest {
        mode: IndexMode::Incremental,
        request_key: None,
        config_fingerprint: DEFAULT_CONFIG_FINGERPRINT.to_owned(),
        changed_paths: watch_changed_paths(identity, trigger, paths),
    };
    let adapter = StorageIndexAdapter::new(storage.as_ref());
    let factory = ParserFactory;
    let service = IndexService::new(
        SourceReader::new(identity.clone(), ScanPolicy::default()),
        &adapter,
        &factory,
        IndexLimits::default(),
    )
    .map_err(indexing_error)?;
    let created = service.prepare(&request).map_err(indexing_error)?;
    if created.deduplicated {
        return Err(internal_error(
            "watch reconciliation unexpectedly deduplicated an unkeyed index job",
        ));
    }
    let job = created.job;
    let job_key = job.id.as_str().to_owned();
    let job_cancellation = CancellationContext::default();
    runtime
        .active_jobs
        .lock()
        .map_err(|_| internal_error("active job registry is poisoned"))?
        .insert(
            job_key.clone(),
            ActiveJob {
                cancellation: job_cancellation.clone(),
                storage: Arc::clone(storage),
                request_key: None,
                job: job.clone(),
                repository_id: identity.id().as_str().to_owned(),
                origin: ActiveJobOrigin::Watch,
            },
        );
    if watcher_cancellation.is_cancelled() {
        job_cancellation.cancel();
    }
    tracing::debug!(
        trigger = trigger.as_str(),
        job_id = job_key,
        "watch reconciliation started"
    );
    let result = service.run_prepared(job, request, &job_cancellation);
    let refresh = storage
        .index_summary()
        .map_err(storage_error)
        .and_then(|summary| refresh_cache(runtime, identity, storage.as_ref(), summary));
    if let Ok(mut active_jobs) = runtime.active_jobs.lock() {
        active_jobs.remove(&job_key);
    }
    match result {
        Ok(_) => refresh,
        Err(IndexingError::Cancelled) if watcher_cancellation.is_cancelled() => Ok(()),
        Err(error) => Err(indexing_error(error)),
    }
}

fn watch_changed_paths(
    identity: &RepositoryIdentity,
    trigger: ReconcileTrigger,
    paths: Vec<PathBuf>,
) -> Option<Vec<RelativeSourcePath>> {
    if trigger != ReconcileTrigger::Event || paths.is_empty() {
        return None;
    }
    let mut changed = BTreeSet::new();
    for path in paths {
        let relative = path.strip_prefix(identity.root().as_path()).ok()?;
        let relative = RelativeSourcePath::from_path(relative).ok()?;
        LanguageRegistry::detect(relative.as_path()).ok()?;
        changed.insert(relative);
    }
    (!changed.is_empty()).then(|| changed.into_iter().collect())
}

impl ToolBackend for McpBackend {
    fn execute(&self, request: BackendRequest) -> Result<ApplicationEnvelope, ApplicationError> {
        match request {
            BackendRequest::RepositoryStatus(_) => self.repository_status(),
            BackendRequest::IndexRepository(request) => self.index_repository(request),
            BackendRequest::JobStatus(request) => self.job_status(request),
            BackendRequest::CancelJob(request) => self.cancel_job(request),
            BackendRequest::SearchSymbols(request) => self.search_symbols(request),
            BackendRequest::GetSymbol(request) => self.get_symbol(request),
            BackendRequest::FindReferences(request) => self.find_references(request),
            BackendRequest::TraceCalls(request) => self.trace_calls(request),
            BackendRequest::GetFileOutline(request) => self.get_file_outline(request),
            BackendRequest::ReadCode(request) => self.read_code(request),
            BackendRequest::GetRepoMap(request) => self.get_repo_map(request),
            BackendRequest::AnalyzeImpact(request) => self.analyze_impact(request),
            BackendRequest::BuildContext(request) => self.build_context(request),
            BackendRequest::SearchMemories(request) => self.search_memories(request),
            BackendRequest::UpsertMemory(request) => self.upsert_memory(request),
            BackendRequest::ForgetMemory(request) => self.forget_memory(request),
        }
    }

    fn read_resource(
        &self,
        request: BackendResourceRequest,
    ) -> Result<ApplicationEnvelope, ApplicationError> {
        match request {
            BackendResourceRequest::RepositoryStatus => self.repository_status(),
            BackendResourceRequest::RepositoryMap => self.get_repo_map(GetRepoMapRequest {
                scope: None,
                depth: 2,
                limit: 200,
                max_bytes: RESOURCE_RESPONSE_BYTES,
            }),
            BackendResourceRequest::Symbol(symbol_id) => self.get_symbol(GetSymbolRequest {
                symbol_id,
                generation_id: None,
                max_bytes: RESOURCE_RESPONSE_BYTES,
            }),
            BackendResourceRequest::Memory(memory_id) => self.memory_resource(&memory_id),
        }
    }

    fn memory_writes_enabled(&self) -> bool {
        self.memory_writes_enabled
    }

    fn shutdown(&self) -> Result<(), ApplicationError> {
        let mut watcher = self
            .runtime
            .watcher
            .lock()
            .map_err(|_| internal_error("watcher registry is poisoned"))?
            .take();
        if let Some(watcher) = &watcher {
            watcher.request_stop();
        }
        for cancellation in self
            .runtime
            .active_jobs
            .lock()
            .map_err(|_| internal_error("active job registry is poisoned"))?
            .values()
        {
            cancellation.cancellation.cancel();
        }
        if let Some(watcher) = &mut watcher {
            watcher.join().map_err(watch_error)?;
        }
        let workers = std::mem::take(
            &mut *self
                .runtime
                .workers
                .lock()
                .map_err(|_| internal_error("index worker registry is poisoned"))?,
        );
        for worker in workers {
            worker
                .join()
                .map_err(|_| internal_error("index worker panicked during shutdown"))?;
        }
        self.runtime
            .owner_storage
            .lock()
            .map_err(|_| internal_error("owner storage registry is poisoned"))?
            .take();
        Ok(())
    }
}

fn refresh_cache(
    runtime: &RuntimeState,
    identity: &RepositoryIdentity,
    storage: &Storage,
    summary: IndexSummary,
) -> Result<(), ApplicationError> {
    let latest_job = summary.latest_job.as_ref().map(job_value);
    let coverage = storage
        .read_snapshot()
        .ok()
        .and_then(|snapshot| snapshot.retrieval_coverage().ok())
        .map_or_else(|| json!({}), coverage_value);
    let mut cached = runtime
        .cached
        .lock()
        .map_err(|_| internal_error("repository status cache is poisoned"))?;
    cached.lifecycle = if storage.role() == ca_storage::StorageRole::Owner {
        "ready"
    } else {
        "read_only_follower"
    };
    cached.repository_id = Some(identity.id().as_str().to_owned());
    cached.generation_id = summary
        .active_generation_id
        .as_ref()
        .map(|value| value.as_str().to_owned());
    cached.storage_role = Some(storage.role().as_str());
    cached.active_files = summary.active_files;
    cached.active_facts = summary.active_facts;
    cached.active_symbols = summary.active_symbols;
    cached.latest_job = latest_job;
    cached.coverage = coverage;
    Ok(())
}

fn coverage_value(coverage: ca_storage::StoredCoverage) -> Value {
    json!({
        "status": coverage.status,
        "warning_count": coverage.warning_count,
        "unresolved_occurrences": coverage.unresolved_occurrences,
        "resolver_version": coverage.resolver_version,
    })
}

fn watch_value(snapshot: &WatchSnapshot) -> Value {
    json!({
        "enabled": snapshot.enabled,
        "running": snapshot.running,
        "backend": snapshot.backend,
        "pending_reconciliation": snapshot.pending_reconciliation,
        "reconciling": snapshot.reconciling,
        "queue_capacity": snapshot.queue_capacity,
        "events_seen": snapshot.events_seen,
        "coalesced_events": snapshot.coalesced_events,
        "overflow_count": snapshot.overflow_count,
        "reconciliation_count": snapshot.reconciliation_count,
        "event_reconciliations": snapshot.event_reconciliations,
        "periodic_reconciliations": snapshot.periodic_reconciliations,
        "last_trigger": snapshot.last_trigger,
        "last_reconciled_unix_ms": snapshot.last_reconciled_unix_ms,
        "last_reconciliation_duration_ms": snapshot.last_reconciliation_duration_ms,
        "last_error": snapshot.last_error,
        "source_filesystem_guarantees": "degraded_event_delivery_events_are_hints",
        "consistency_strategy": "bounded_queue_plus_periodic_scan_and_content_hash_reconciliation"
    })
}

fn direction(value: DirectionInput) -> RelationDirection {
    match value {
        DirectionInput::Incoming => RelationDirection::Incoming,
        DirectionInput::Outgoing => RelationDirection::Outgoing,
    }
}

fn memory_kind(value: MemoryKindInput) -> MemoryKind {
    match value {
        MemoryKindInput::Decision => MemoryKind::Decision,
        MemoryKindInput::Convention => MemoryKind::Convention,
        MemoryKindInput::Pitfall => MemoryKind::Pitfall,
        MemoryKindInput::Task => MemoryKind::Task,
    }
}

fn memory_value(record: &MemoryRecord) -> Value {
    json!({
        "id": record.id,
        "text": record.text,
        "kind": record.kind.as_str(),
        "revision": record.revision,
        "author": record.author,
        "origin": record.origin,
        "scope": record.scope,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "evidence": record.evidence.iter().map(|item| json!({
            "relative_path": item.relative_path,
            "content_hash": item.content_hash,
            "symbol_id": item.symbol_id,
        })).collect::<Vec<_>>(),
        "evidence_status": record.evidence_status.as_str(),
        "provenance": "explicitly_authored_untrusted_project_note",
        "authoritative_code_fact": false,
    })
}

fn memory_envelope(
    repository_id: &str,
    generation_id: Option<String>,
    status: &str,
    data: Value,
) -> ApplicationEnvelope {
    let mut envelope =
        ApplicationEnvelope::success(Some(repository_id.to_owned()), generation_id, status, data);
    envelope.warnings.push(
        "Project memories are explicitly authored untrusted data, not authoritative code facts"
            .to_owned(),
    );
    envelope
}

fn bounded_memory_search_envelope(
    repository_id: &str,
    generation_id: Option<String>,
    default_excludes_stale: bool,
    mut results: Vec<Value>,
) -> Result<ApplicationEnvelope, ApplicationError> {
    let mut removed = 0_usize;
    loop {
        let mut envelope = memory_envelope(
            repository_id,
            generation_id.clone(),
            "ok",
            json!({
                "source": "explicit_project_notes",
                "authoritative_code_facts": false,
                "default_excludes_stale": default_excludes_stale,
                "results": results,
                "omitted_for_response_budget": removed,
            }),
        );
        envelope.truncated = removed > 0;
        if removed > 0 {
            envelope.warnings.push(format!(
                "{removed} memory result(s) omitted to satisfy the response byte budget"
            ));
        }
        if serde_json::to_vec(&envelope).is_ok_and(|encoded| encoded.len() <= MEMORY_RESPONSE_BYTES)
        {
            return Ok(envelope);
        }
        if results.pop().is_none() {
            return Err(ApplicationError::new(
                "RESPONSE_TOO_LARGE",
                "memory search metadata cannot fit the response budget",
            ));
        }
        removed = removed.saturating_add(1);
    }
}

fn engine_budget(requested: usize) -> Result<usize, ApplicationError> {
    if !(512..=ca_mcp::MAX_MCP_RESPONSE_BYTES).contains(&requested) {
        return Err(ApplicationError::new(
            "INVALID_RESPONSE_BUDGET",
            "max_bytes must be between 512 and 65536",
        ));
    }
    Ok(requested.min(ENGINE_RESPONSE_BYTES))
}

fn validate_range(
    name: &'static str,
    value: usize,
    minimum: usize,
    maximum: usize,
) -> Result<(), ApplicationError> {
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(ApplicationError::new(
            "INVALID_ARGUMENT",
            format!("{name} must be between {minimum} and {maximum}"),
        ))
    }
}

fn validate_graph_limits(
    depth: usize,
    max_nodes: usize,
    max_edges: usize,
    deadline_ms: u64,
) -> Result<(), ApplicationError> {
    validate_range("depth", depth, 0, 8)?;
    validate_range("max_nodes", max_nodes, 1, 500)?;
    validate_range("max_edges", max_edges, 1, 2_000)?;
    if !(1..=30_000).contains(&deadline_ms) {
        return Err(ApplicationError::new(
            "INVALID_ARGUMENT",
            "deadline_ms must be between 1 and 30000",
        ));
    }
    Ok(())
}

fn envelope_from_budgeted<T: Serialize>(
    result: Budgeted<T>,
) -> Result<ApplicationEnvelope, ApplicationError> {
    let data = serde_json::to_value(result.data)
        .map_err(|error| internal_error(format!("cannot serialize retrieval result: {error}")))?;
    let repository_id = data
        .get("repository_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let generation_id = data
        .get("generation_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let coverage = data.get("coverage").cloned().unwrap_or_else(|| json!({}));
    let warnings = data
        .get("warnings")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let truncated = data
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let next_cursor = data
        .get("next_cursor")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(ApplicationEnvelope {
        schema_version: 1,
        repository_id,
        generation_id,
        status: "ok".to_owned(),
        data,
        coverage,
        warnings,
        truncated,
        next_cursor,
    })
}

fn job_envelope(
    repository_id: &str,
    job: &IndexJob,
    deduplicated: bool,
    status: &str,
) -> ApplicationEnvelope {
    ApplicationEnvelope::success(
        Some(repository_id.to_owned()),
        job.generation_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        status,
        json!({
            "job_id": job.id.as_str(),
            "state": index_state(job.state),
            "cancel_requested": job.cancel_requested,
            "deduplicated": deduplicated,
            "progress": index_progress(&job.progress),
            "error_summary": job.error_summary
        }),
    )
}

fn stored_job_envelope(repository_id: &str, job: &JobRecord, status: &str) -> ApplicationEnvelope {
    ApplicationEnvelope::success(
        Some(repository_id.to_owned()),
        job.generation_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        status,
        job_value(job),
    )
}

fn job_value(job: &JobRecord) -> Value {
    json!({
        "job_id": job.id.as_str(),
        "generation_id": job.generation_id.as_ref().map(|value| value.as_str()),
        "mode": job.mode,
        "state": job.state.as_str(),
        "cancel_requested": job.cancel_requested,
        "progress": {
            "files_discovered": job.progress.files_discovered,
            "files_reused": job.progress.files_reused,
            "files_parsed": job.progress.files_parsed,
            "files_failed": job.progress.files_failed,
            "files_persisted": job.progress.files_persisted,
            "files_deleted": job.progress.files_deleted,
            "warning_count": job.progress.warning_count,
        },
        "error_summary": job.error_summary,
        "created_at": job.created_at,
        "updated_at": job.updated_at
    })
}

fn index_progress(progress: &IndexProgress) -> Value {
    json!({
        "files_discovered": progress.files_discovered,
        "files_reused": progress.files_reused,
        "files_parsed": progress.files_parsed,
        "files_failed": progress.files_failed,
        "files_persisted": progress.files_persisted,
        "files_deleted": progress.files_deleted,
        "warning_count": progress.warning_count,
    })
}

fn index_state(state: IndexJobState) -> &'static str {
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

fn storage_error(error: StorageError) -> ApplicationError {
    match error {
        StorageError::WriterBusy { retry_after_ms } => ApplicationError::new(
            "WRITER_BUSY",
            format!("repository index writer is owned by another process; retry after {retry_after_ms} ms"),
        )
        .retryable(Some(retry_after_ms)),
        StorageError::QueueFull => ApplicationError::new(
            "WRITER_QUEUE_FULL",
            "repository writer queue is full; retry the bounded operation",
        )
        .retryable(Some(250)),
        StorageError::WriterTimeout { .. } | StorageError::WriterDisconnected => {
            ApplicationError::new("WRITER_UNAVAILABLE", error.to_string()).retryable(Some(500))
        }
        StorageError::IndexNotReady => {
            ApplicationError::new("INDEX_NOT_READY", "no active index generation exists")
        }
        StorageError::JobNotFound(_) => {
            ApplicationError::new("JOB_NOT_FOUND", error.to_string())
        }
        StorageError::MemoryNotFound(_) => {
            ApplicationError::new("MEMORY_NOT_FOUND", error.to_string())
        }
        StorageError::MemoryRevisionRequired { .. }
        | StorageError::MemoryRevisionConflict { .. } => {
            ApplicationError::new("MEMORY_REVISION_CONFLICT", error.to_string())
        }
        StorageError::MemoryLimitExceeded { .. } => {
            ApplicationError::new("MEMORY_LIMIT_EXCEEDED", error.to_string())
        }
        StorageError::MemoryEvidenceInvalid(_) => {
            ApplicationError::new("MEMORY_EVIDENCE_INVALID", error.to_string())
        }
        StorageError::RootIdentityMismatch { .. } => {
            ApplicationError::new("CROSS_REPOSITORY_ID", error.to_string())
        }
        StorageError::InvalidInput(_) => {
            ApplicationError::new("INVALID_ARGUMENT", error.to_string())
        }
        _ => ApplicationError::new("STORAGE_ERROR", error.to_string()),
    }
}

fn memory_error(error: MemoryError<StorageError>) -> ApplicationError {
    match error {
        MemoryError::InvalidLimits => {
            ApplicationError::new("INVALID_ARGUMENT", "invalid memory limits")
        }
        MemoryError::InvalidInput(message) => ApplicationError::new("INVALID_ARGUMENT", message),
        MemoryError::Store(error) => storage_error(error),
    }
}

fn retrieval_error(error: RetrievalError) -> ApplicationError {
    let code = match error {
        RetrievalError::InvalidLimits
        | RetrievalError::InvalidQuery { .. }
        | RetrievalError::InvalidResultLimit
        | RetrievalError::InvalidResponseBudget
        | RetrievalError::InvalidRange => "INVALID_ARGUMENT",
        RetrievalError::InvalidCursor | RetrievalError::CursorQueryMismatch => "INVALID_CURSOR",
        RetrievalError::ForeignCursor => "CROSS_REPOSITORY_ID",
        RetrievalError::StaleCursor => "STALE_CURSOR",
        RetrievalError::SymbolNotFound(_) => "SYMBOL_NOT_FOUND",
        RetrievalError::FileNotIndexed(_) => "FILE_NOT_INDEXED",
        RetrievalError::ContentChanged(_) => "CONTENT_CHANGED",
        RetrievalError::SourceDeleted(_) => "SOURCE_DELETED",
        RetrievalError::SourceExcluded(_) => "SOURCE_EXCLUDED",
        RetrievalError::SourceAccess { .. } => "SOURCE_ACCESS_FAILED",
        RetrievalError::DeadlineExceeded => "DEADLINE_EXCEEDED",
        RetrievalError::Store(_) => "STORAGE_ERROR",
        RetrievalError::ResponseTooLarge => "RESPONSE_TOO_LARGE",
        RetrievalError::Serialization(_) => "SERIALIZATION_FAILED",
    };
    ApplicationError::new(code, error.to_string())
}

fn indexing_error(error: IndexingError) -> ApplicationError {
    let code = match error {
        IndexingError::InvalidLimits => "INVALID_ARGUMENT",
        IndexingError::AlreadyRunning(_) => "INDEX_ALREADY_RUNNING",
        IndexingError::DeduplicatedTerminal { .. } => "REQUEST_ALREADY_TERMINAL",
        IndexingError::Cancelled => "CANCELLED",
        IndexingError::ScanIncomplete(_) => "SCAN_INCOMPLETE",
        IndexingError::Store(_) => "STORAGE_ERROR",
        IndexingError::Extractor(_) => "EXTRACTOR_ERROR",
        IndexingError::WorkerDisconnected | IndexingError::WorkerPanicked => "WORKER_FAILED",
        IndexingError::Resolution(_) => "RESOLUTION_FAILED",
        IndexingError::Clock => "CLOCK_ERROR",
    };
    ApplicationError::new(code, error.to_string())
}

fn repository_error(error: RepositoryError) -> ApplicationError {
    ApplicationError::new("REPOSITORY_ACCESS_FAILED", error.to_string())
}

fn core_error(error: ca_core::CoreError) -> ApplicationError {
    ApplicationError::new("INVALID_ARGUMENT", error.to_string())
}

fn internal_error(message: impl Into<String>) -> ApplicationError {
    ApplicationError::new("INTERNAL_ERROR", message)
}

fn watch_error(error: WatchError) -> ApplicationError {
    ApplicationError::new("WATCH_ERROR", error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::*;

    #[test]
    fn watcher_reconciliation_does_not_reject_a_manual_index_request() {
        assert!(!manual_index_is_blocked([ActiveJobOrigin::Watch]));
        assert!(manual_index_is_blocked([ActiveJobOrigin::Manual]));
        assert!(manual_index_is_blocked([
            ActiveJobOrigin::Watch,
            ActiveJobOrigin::Manual,
        ]));
    }

    #[test]
    fn watch_events_keep_only_bounded_source_hints_and_fallback_for_other_changes() {
        let nonce = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codeatlas-watch-hints-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).expect("create test root");
        let identity =
            RepositoryIdentity::derive(RepositoryRoot::new(&root).expect("authorize test root"))
                .expect("derive test identity");
        let canonical_root = identity.root().as_path();

        let hints = watch_changed_paths(
            &identity,
            ReconcileTrigger::Event,
            vec![
                canonical_root.join("src/lib.rs"),
                canonical_root.join("src/lib.rs"),
            ],
        )
        .expect("source-only event remains targeted");
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].as_str(), "src/lib.rs");
        assert!(
            watch_changed_paths(
                &identity,
                ReconcileTrigger::Event,
                vec![canonical_root.join("go.mod")],
            )
            .is_none()
        );
        assert!(
            watch_changed_paths(
                &identity,
                ReconcileTrigger::Overflow,
                vec![canonical_root.join("src/lib.rs")],
            )
            .is_none()
        );

        fs::remove_dir_all(root).expect("remove test root");
    }
}
