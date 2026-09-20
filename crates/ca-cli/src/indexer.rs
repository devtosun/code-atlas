use std::{collections::BTreeMap, sync::OnceLock};

use ca_core::{CancellationContext, GenerationId, JobId};
use ca_engine::{
    indexing::{
        ActiveFileVersion, CreatedJob, ExtractionFailure, ExtractionFingerprints, ExtractionWorker,
        ExtractionWorkerFactory, IndexJob, IndexJobState, IndexMode, IndexProgress, IndexStore,
        IndexedCategory, IndexedDiagnostic, IndexedFile, IndexedObservation,
    },
    repository::SourceFile,
    resolution::{
        EdgeTarget, ObservationKind, ResolutionFile, ResolutionGraph, ResolutionInput,
        ResolutionObservation,
    },
};
use ca_languages::{
    DiagnosticSeverity, ExtractionResult, LanguageError, LanguageId, LanguageRegistry, ParseLimits,
    ParserWorker, ResolutionCategory, SyntaxObservation,
};
use ca_storage::{
    ActiveFile, CreatedJob as StoredCreatedJob, GenerationCompletion, GenerationPlan, JobProgress,
    JobRecord, JobState, NewJob, ObservationCategory, SearchDocument, StagedFile, Storage,
    StorageError, StoredDiagnostic, StoredEdgeTarget, StoredExternalNode, StoredFact,
    StoredLocalSymbolRef, StoredObservation, StoredResolutionDiagnostic, StoredResolutionGraph,
    StoredResolvedEdge,
};

pub const DEFAULT_CONFIG_FINGERPRINT: &str = "codeatlas-default-index-config-v2-search-tokens";

#[derive(Debug, Default)]
pub struct ParserFactory;

impl ExtractionWorkerFactory for ParserFactory {
    type Worker = LanguageExtractionWorker;

    fn create(&self) -> Result<Self::Worker, ExtractionFailure> {
        Ok(LanguageExtractionWorker {
            parser: ParserWorker::new(),
        })
    }

    fn fingerprints(
        &self,
        path: &ca_engine::repository::RelativeSourcePath,
    ) -> Result<ExtractionFingerprints, ExtractionFailure> {
        let provider = LanguageRegistry::detect(path.as_path()).map_err(language_failure)?;
        cached_fingerprints()
            .get(&provider.id)
            .cloned()
            .ok_or_else(|| ExtractionFailure {
                cancelled: false,
                message: format!("fingerprints are unavailable for {}", provider.id),
            })
    }

    fn extractor_set_fingerprint(&self) -> String {
        static FINGERPRINT: OnceLock<String> = OnceLock::new();
        FINGERPRINT
            .get_or_init(|| {
                let mut hasher = blake3::Hasher::new();
                for provider in LanguageRegistry::providers() {
                    hasher.update(provider.id.as_str().as_bytes());
                    hasher.update(&[0]);
                    if let Some(fingerprints) = cached_fingerprints().get(&provider.id) {
                        hasher.update(fingerprints.extractor.as_bytes());
                    }
                    hasher.update(&[0xff]);
                }
                hasher.finalize().to_hex().to_string()
            })
            .clone()
    }
}

fn cached_fingerprints() -> &'static BTreeMap<LanguageId, ExtractionFingerprints> {
    static FINGERPRINTS: OnceLock<BTreeMap<LanguageId, ExtractionFingerprints>> = OnceLock::new();
    FINGERPRINTS.get_or_init(|| {
        LanguageRegistry::providers()
            .iter()
            .map(|provider| {
                (
                    provider.id,
                    ExtractionFingerprints {
                        grammar: provider.grammar_fingerprint(),
                        query: provider.query_fingerprint(),
                        extractor: provider.extractor_fingerprint(),
                    },
                )
            })
            .collect()
    })
}

pub struct LanguageExtractionWorker {
    parser: ParserWorker,
}

impl ExtractionWorker for LanguageExtractionWorker {
    fn extract(
        &mut self,
        source: &SourceFile,
        cancellation: &CancellationContext,
    ) -> Result<IndexedFile, ExtractionFailure> {
        let provider =
            LanguageRegistry::detect(source.relative_path().as_path()).map_err(language_failure)?;
        match self.parser.parse_file(
            provider.id,
            source.relative_path().as_path(),
            source.bytes(),
            ParseLimits::DEFAULT,
            cancellation,
        ) {
            Ok(result) => extraction_to_file(source, result),
            Err(LanguageError::Cancelled { .. }) => Err(ExtractionFailure {
                cancelled: true,
                message: "parser observed cooperative cancellation".to_owned(),
            }),
            Err(error) => Ok(failed_file(source, provider, error)),
        }
    }
}

fn extraction_to_file(
    source: &SourceFile,
    result: ExtractionResult,
) -> Result<IndexedFile, ExtractionFailure> {
    let coverage = serde_json::to_string(&serde_json::json!({
        "root_has_error": result.coverage.root_has_error,
        "syntax_nodes_visited": result.coverage.syntax_nodes_visited,
        "syntax_traversal_truncated": result.coverage.syntax_traversal_truncated,
        "captures_truncated": result.coverage.captures_truncated,
        "query_match_limit_exceeded": result.coverage.query_match_limit_exceeded,
        "resolution": "syntax_only"
    }))
    .map_err(json_failure)?;
    let parse_status = if result.coverage.syntax_traversal_truncated
        || result.coverage.captures_truncated
        || result.coverage.query_match_limit_exceeded
        || result.coverage.root_has_error
    {
        "partial"
    } else {
        "complete"
    };
    let mut observations = Vec::new();
    append_observations(
        &mut observations,
        IndexedCategory::Symbol,
        result.declarations,
    );
    append_observations(&mut observations, IndexedCategory::Scope, result.scopes);
    append_observations(&mut observations, IndexedCategory::Import, result.imports);
    append_observations(
        &mut observations,
        IndexedCategory::Reference,
        result.references,
    );
    append_observations(
        &mut observations,
        IndexedCategory::CallSite,
        result.call_sites,
    );
    append_observations(
        &mut observations,
        IndexedCategory::Condition,
        result.conditions,
    );
    let diagnostics = result
        .diagnostics
        .into_iter()
        .map(|diagnostic| IndexedDiagnostic {
            code: diagnostic.code,
            message: diagnostic.message,
            severity: match diagnostic.severity {
                DiagnosticSeverity::Information => "information",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Error => "error",
            }
            .to_owned(),
            byte_range: diagnostic.range.map(|range| range.bytes),
        })
        .collect();
    Ok(IndexedFile {
        relative_path: source.relative_path().as_str().to_owned(),
        content_hash: result.source_hash,
        byte_length: u64::try_from(source.bytes().len()).unwrap_or(u64::MAX),
        language: result.language.as_str().to_owned(),
        grammar_fingerprint: result.grammar_fingerprint,
        query_fingerprint: result.query_fingerprint,
        extractor_fingerprint: result.extractor_fingerprint,
        parse_status: parse_status.to_owned(),
        coverage,
        observations,
        diagnostics,
    })
}

fn append_observations(
    target: &mut Vec<IndexedObservation>,
    category: IndexedCategory,
    values: Vec<SyntaxObservation>,
) {
    target.extend(values.into_iter().map(|observation| {
        IndexedObservation {
            category,
            id: observation.id,
            kind: observation.kind,
            spelling: observation.spelling,
            byte_range: observation.range.bytes,
            syntax_range: observation.syntax_range.bytes,
            scope_id: observation.scope_id,
            container: observation.container,
            signature: observation.signature,
            receiver: observation.receiver_type,
            alias: observation.alias,
            target_id: observation.target_id,
            resolution: match observation.resolution {
                ResolutionCategory::SyntaxObservation => "syntax_observation",
                ResolutionCategory::Unresolved => "unresolved",
                ResolutionCategory::LexicallyResolved => "lexically_resolved",
            }
            .to_owned(),
            attributes: observation.attributes,
            limitations: observation.limitations,
        }
    }));
}

fn failed_file(
    source: &SourceFile,
    provider: ca_languages::LanguageProvider,
    error: LanguageError,
) -> IndexedFile {
    IndexedFile {
        relative_path: source.relative_path().as_str().to_owned(),
        content_hash: source.content_hash().to_owned(),
        byte_length: u64::try_from(source.bytes().len()).unwrap_or(u64::MAX),
        language: provider.id.as_str().to_owned(),
        grammar_fingerprint: provider.grammar_fingerprint(),
        query_fingerprint: provider.query_fingerprint(),
        extractor_fingerprint: provider.extractor_fingerprint(),
        parse_status: "failed".to_owned(),
        coverage: "{\"resolution\":\"syntax_only\",\"parse_failed\":true}".to_owned(),
        observations: Vec::new(),
        diagnostics: vec![IndexedDiagnostic {
            code: "parser_error".to_owned(),
            message: error.to_string(),
            severity: "error".to_owned(),
            byte_range: None,
        }],
    }
}

fn language_failure(error: LanguageError) -> ExtractionFailure {
    ExtractionFailure {
        cancelled: matches!(error, LanguageError::Cancelled { .. }),
        message: error.to_string(),
    }
}

fn json_failure(error: serde_json::Error) -> ExtractionFailure {
    ExtractionFailure {
        cancelled: false,
        message: format!("cannot encode parse coverage: {error}"),
    }
}

pub struct StorageIndexAdapter<'a> {
    storage: &'a Storage,
}

impl<'a> StorageIndexAdapter<'a> {
    pub const fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }
}

impl IndexStore for StorageIndexAdapter<'_> {
    type Error = StorageError;

    fn create_job(
        &self,
        mode: IndexMode,
        request_key: Option<&str>,
    ) -> Result<CreatedJob, Self::Error> {
        let StoredCreatedJob { job, deduplicated } = self.storage.create_job(NewJob {
            mode: mode.as_str().to_owned(),
            request_key: request_key.map(str::to_owned),
            owner_instance_id: format!("cli-{}", std::process::id()),
        })?;
        Ok(CreatedJob {
            job: map_job(job),
            deduplicated,
        })
    }

    fn update_job(
        &self,
        job_id: &JobId,
        state: IndexJobState,
        progress: &IndexProgress,
        error_summary: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.storage.update_job(
            job_id.clone(),
            map_state_to_storage(state),
            map_progress_to_storage(progress),
            error_summary.map(str::to_owned),
        )
    }

    fn job_status(&self, job_id: &JobId) -> Result<IndexJob, Self::Error> {
        self.storage.job_status(job_id).map(map_job)
    }

    fn active_files(&self) -> Result<Vec<ActiveFileVersion>, Self::Error> {
        self.storage
            .active_files()
            .map(|files| files.into_iter().map(map_active_file).collect())
    }

    fn begin_generation(
        &self,
        job_id: &JobId,
        generation_id: &GenerationId,
        config_fingerprint: &str,
        extractor_set_fingerprint: &str,
        reuse_parent: bool,
        deleted_paths: Vec<String>,
    ) -> Result<(), Self::Error> {
        self.storage.begin_generation(GenerationPlan {
            generation_id: generation_id.clone(),
            job_id: job_id.clone(),
            config_hash: config_fingerprint.to_owned(),
            extractor_set_hash: extractor_set_fingerprint.to_owned(),
            reuse_parent,
            deleted_paths,
        })
    }

    fn reuse_files(
        &self,
        generation_id: &GenerationId,
        relative_paths: Vec<String>,
    ) -> Result<(), Self::Error> {
        self.storage
            .reuse_files(generation_id.clone(), relative_paths)
    }

    fn stage_files(
        &self,
        generation_id: &GenerationId,
        files: Vec<IndexedFile>,
    ) -> Result<(), Self::Error> {
        let files = files
            .into_iter()
            .map(indexed_to_staged)
            .collect::<Result<Vec<_>, _>>()?;
        self.storage.stage_files(generation_id.clone(), files)
    }

    fn resolution_input(
        &self,
        generation_id: &GenerationId,
    ) -> Result<ResolutionInput, Self::Error> {
        let files = self
            .storage
            .generation_resolution_files(generation_id)?
            .into_iter()
            .map(|file| {
                let observations = file
                    .observations
                    .into_iter()
                    .map(stored_to_resolution_observation)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ResolutionFile {
                    relative_path: file.relative_path,
                    language: file.language,
                    observations,
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;
        Ok(ResolutionInput {
            generation_id: generation_id.clone(),
            files,
            manifests: Vec::new(),
        })
    }

    fn replace_resolution_graph(
        &self,
        generation_id: &GenerationId,
        graph: ResolutionGraph,
    ) -> Result<(), Self::Error> {
        if graph.generation_id != *generation_id {
            return Err(StorageError::InvalidInput(
                "resolution graph generation does not match the index job".to_owned(),
            ));
        }
        self.storage
            .replace_resolution_graph(resolution_graph_to_stored(graph)?)
    }

    fn complete_generation(
        &self,
        generation_id: &GenerationId,
        scan_complete: bool,
        coverage_status: &str,
        warning_count: u64,
        failure_summary: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.storage.complete_generation(GenerationCompletion {
            generation_id: generation_id.clone(),
            scan_complete,
            coverage_status: coverage_status.to_owned(),
            warning_count,
            failure_summary: failure_summary.map(str::to_owned),
        })
    }

    fn activate_generation(&self, generation_id: &GenerationId) -> Result<(), Self::Error> {
        self.storage.activate_generation(generation_id.clone())
    }

    fn abandon_generation(
        &self,
        generation_id: Option<&GenerationId>,
        job_id: &JobId,
        state: IndexJobState,
        progress: &IndexProgress,
        error_summary: &str,
    ) -> Result<(), Self::Error> {
        self.storage.abandon_generation(
            generation_id.cloned(),
            job_id.clone(),
            map_state_to_storage(state),
            map_progress_to_storage(progress),
            error_summary.to_owned(),
        )
    }

    fn gc_abandoned(&self) -> Result<(), Self::Error> {
        self.storage.gc_abandoned()
    }
}

fn stored_to_resolution_observation(
    observation: StoredObservation,
) -> Result<ResolutionObservation, StorageError> {
    let category = match observation.category {
        ObservationCategory::Symbol => ObservationKind::Symbol,
        ObservationCategory::Scope => ObservationKind::Scope,
        ObservationCategory::Import => ObservationKind::Import,
        ObservationCategory::Reference => ObservationKind::Reference,
        ObservationCategory::CallSite => ObservationKind::CallSite,
        ObservationCategory::Condition => {
            return Err(StorageError::InvalidInput(
                "conditions are not resolver observations".to_owned(),
            ));
        }
    };
    Ok(ResolutionObservation {
        category,
        id: observation.id,
        kind: observation.kind,
        spelling: observation.spelling,
        byte_range: observation.byte_range,
        syntax_range: observation.syntax_range,
        scope_id: observation.scope_id,
        container: observation.container,
        signature: observation.signature,
        receiver: observation.receiver,
        alias: observation.alias,
        attributes: serde_json::from_str(&observation.attributes_json)
            .map_err(storage_json_error)?,
        limitations: serde_json::from_str(&observation.limitations_json)
            .map_err(storage_json_error)?,
    })
}

fn resolution_graph_to_stored(
    graph: ResolutionGraph,
) -> Result<StoredResolutionGraph, StorageError> {
    let unresolved_count = u64::try_from(graph.stats.unresolved).map_err(|_| {
        StorageError::InvalidInput("unresolved count exceeds storage bounds".to_owned())
    })?;
    let edges = graph
        .edges
        .into_iter()
        .map(|edge| {
            let target = edge.target.map(|target| match target {
                EdgeTarget::LocalFile { relative_path } => {
                    StoredEdgeTarget::LocalFile { relative_path }
                }
                EdgeTarget::LocalSymbol(target) => {
                    StoredEdgeTarget::LocalSymbol(StoredLocalSymbolRef {
                        relative_path: target.relative_path,
                        symbol_id: target.symbol_id,
                    })
                }
                EdgeTarget::External { node_id } => StoredEdgeTarget::External { node_id },
            });
            Ok(StoredResolvedEdge {
                id: edge.id,
                relationship: edge.relationship.as_str().to_owned(),
                source_path: edge.source_path,
                source_observation_id: edge.source_observation_id,
                source_category: edge.source_category.as_str().to_owned(),
                source_range: edge.source_range,
                source_symbol_id: edge.source_symbol.map(|symbol| symbol.symbol_id),
                target,
                resolution: edge.resolution.as_str().to_owned(),
                rule_version: edge.rule,
                resolver_version: edge.resolver_version,
                candidate_count: u64::try_from(edge.candidate_count).map_err(|_| {
                    StorageError::InvalidInput("candidate count exceeds storage bounds".to_owned())
                })?,
                reason: edge.reason,
                evidence_json: serde_json::to_string(&edge.evidence).map_err(storage_json_error)?,
                limitations_json: serde_json::to_string(&edge.limitations)
                    .map_err(storage_json_error)?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(StoredResolutionGraph {
        generation_id: graph.generation_id,
        resolver_version: graph.resolver_version,
        edges,
        external_nodes: graph
            .external_nodes
            .into_iter()
            .map(|node| StoredExternalNode {
                id: node.id,
                language: node.language,
                kind: node.kind,
                label: node.label,
            })
            .collect(),
        diagnostics: graph
            .diagnostics
            .into_iter()
            .map(|diagnostic| StoredResolutionDiagnostic {
                relative_path: diagnostic.relative_path,
                observation_id: diagnostic.observation_id,
                code: diagnostic.code,
                message: diagnostic.message,
            })
            .collect(),
        unresolved_count,
    })
}

fn indexed_to_staged(file: IndexedFile) -> Result<StagedFile, StorageError> {
    let mut facts = Vec::new();
    let mut search_documents = Vec::new();
    for observation in &file.observations {
        if observation.category == IndexedCategory::Symbol {
            facts.push(StoredFact {
                id: observation.id.clone(),
                kind: observation.kind.clone(),
                name: observation.spelling.clone(),
                byte_range: observation.byte_range,
            });
            search_documents.push(SearchDocument {
                name: observation.spelling.clone(),
                content: format!(
                    "{} {} {} {}",
                    observation.kind,
                    observation.spelling,
                    ca_engine::retrieval::identifier_tokens(&observation.spelling).join(" "),
                    observation.signature.as_deref().unwrap_or("")
                ),
            });
        }
    }
    let observations = file
        .observations
        .into_iter()
        .map(|observation| {
            Ok(StoredObservation {
                category: match observation.category {
                    IndexedCategory::Symbol => ObservationCategory::Symbol,
                    IndexedCategory::Scope => ObservationCategory::Scope,
                    IndexedCategory::Import => ObservationCategory::Import,
                    IndexedCategory::Reference => ObservationCategory::Reference,
                    IndexedCategory::CallSite => ObservationCategory::CallSite,
                    IndexedCategory::Condition => ObservationCategory::Condition,
                },
                id: observation.id,
                kind: observation.kind,
                spelling: observation.spelling,
                byte_range: observation.byte_range,
                syntax_range: observation.syntax_range,
                scope_id: observation.scope_id,
                container: observation.container,
                signature: observation.signature,
                receiver: observation.receiver,
                alias: observation.alias,
                target_id: observation.target_id,
                resolution: observation.resolution,
                attributes_json: serde_json::to_string(&observation.attributes)
                    .map_err(storage_json_error)?,
                limitations_json: serde_json::to_string(&observation.limitations)
                    .map_err(storage_json_error)?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    let diagnostics = file
        .diagnostics
        .into_iter()
        .map(|diagnostic| StoredDiagnostic {
            code: diagnostic.code,
            message: diagnostic.message,
            severity: diagnostic.severity,
            byte_range: diagnostic.byte_range,
        })
        .collect();
    Ok(StagedFile {
        relative_path: file.relative_path,
        content_hash: file.content_hash,
        extractor_hash: file.extractor_fingerprint,
        language: file.language,
        grammar_hash: file.grammar_fingerprint,
        query_hash: file.query_fingerprint,
        parse_status: file.parse_status,
        coverage_json: file.coverage,
        byte_length: file.byte_length,
        facts,
        observations,
        diagnostics,
        search_documents,
    })
}

fn storage_json_error(error: serde_json::Error) -> StorageError {
    StorageError::InvalidInput(format!("cannot encode extraction metadata: {error}"))
}

fn map_job(job: JobRecord) -> IndexJob {
    IndexJob {
        id: job.id,
        generation_id: job.generation_id,
        state: map_state_from_storage(job.state),
        cancel_requested: job.cancel_requested,
        progress: IndexProgress {
            files_discovered: job.progress.files_discovered,
            files_reused: job.progress.files_reused,
            files_parsed: job.progress.files_parsed,
            files_failed: job.progress.files_failed,
            files_persisted: job.progress.files_persisted,
            files_deleted: job.progress.files_deleted,
            warning_count: job.progress.warning_count,
        },
        error_summary: job.error_summary,
    }
}

fn map_progress_to_storage(progress: &IndexProgress) -> JobProgress {
    JobProgress {
        files_discovered: progress.files_discovered,
        files_reused: progress.files_reused,
        files_parsed: progress.files_parsed,
        files_failed: progress.files_failed,
        files_persisted: progress.files_persisted,
        files_deleted: progress.files_deleted,
        warning_count: progress.warning_count,
    }
}

fn map_state_from_storage(state: JobState) -> IndexJobState {
    match state {
        JobState::Queued => IndexJobState::Queued,
        JobState::Scanning => IndexJobState::Scanning,
        JobState::Parsing => IndexJobState::Parsing,
        JobState::Resolving => IndexJobState::Resolving,
        JobState::Committing => IndexJobState::Committing,
        JobState::Completed => IndexJobState::Completed,
        JobState::Cancelled => IndexJobState::Cancelled,
        JobState::Failed => IndexJobState::Failed,
        JobState::Interrupted => IndexJobState::Interrupted,
    }
}

fn map_state_to_storage(state: IndexJobState) -> JobState {
    match state {
        IndexJobState::Queued => JobState::Queued,
        IndexJobState::Scanning => JobState::Scanning,
        IndexJobState::Parsing => JobState::Parsing,
        IndexJobState::Resolving => JobState::Resolving,
        IndexJobState::Committing => JobState::Committing,
        IndexJobState::Completed => JobState::Completed,
        IndexJobState::Cancelled => JobState::Cancelled,
        IndexJobState::Failed => JobState::Failed,
        IndexJobState::Interrupted => JobState::Interrupted,
    }
}

fn map_active_file(file: ActiveFile) -> ActiveFileVersion {
    ActiveFileVersion {
        relative_path: file.relative_path,
        content_hash: file.content_hash,
        grammar_fingerprint: file.grammar_hash,
        query_fingerprint: file.query_hash,
        extractor_fingerprint: file.extractor_hash,
        config_fingerprint: file.config_hash,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use ca_engine::{
        indexing::{IndexLimits, IndexRequest, IndexService, IndexingError},
        repository::{RepositoryIdentity, ScanPolicy, SourceReader},
    };
    use ca_storage::{CancelJobResult, StoragePaths};
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct ResolutionManifest {
        schema_version: u32,
        corpus_root: String,
        labels: Vec<ResolutionLabel>,
    }

    #[derive(Debug, Deserialize)]
    struct ResolutionLabel {
        language: String,
        source_path: String,
        source_category: String,
        source_spelling: String,
        occurrence: usize,
        relationship: String,
        resolution: String,
        candidate_count: u64,
        targets: Vec<ExpectedTarget>,
    }

    #[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
    struct ExpectedTarget {
        path: String,
        name: Option<String>,
    }

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
    struct Fraction {
        numerator: u64,
        denominator: u64,
    }

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
    struct LanguageMetrics {
        language: String,
        labelled_sites: u64,
        syntax_coverage: Fraction,
        binding_precision: Fraction,
        binding_recall: Fraction,
        unresolved_rate: Fraction,
        candidate_sites: u64,
        external_observation_sites: u64,
    }

    #[derive(Debug, Deserialize)]
    struct EvaluationArtifact {
        schema_version: u32,
        corpus: String,
        metrics: Vec<LanguageMetrics>,
    }

    #[derive(Default)]
    struct MetricAccumulator {
        sites: u64,
        found: u64,
        predicted: u64,
        expected: u64,
        correct: u64,
        unresolved: u64,
        candidates: u64,
        external: u64,
    }

    struct SlowFactory {
        started: Arc<AtomicBool>,
        observed_cancel: Arc<AtomicBool>,
    }

    struct SlowWorker {
        started: Arc<AtomicBool>,
        observed_cancel: Arc<AtomicBool>,
    }

    impl ExtractionWorkerFactory for SlowFactory {
        type Worker = SlowWorker;

        fn create(&self) -> Result<Self::Worker, ExtractionFailure> {
            Ok(SlowWorker {
                started: Arc::clone(&self.started),
                observed_cancel: Arc::clone(&self.observed_cancel),
            })
        }

        fn fingerprints(
            &self,
            _path: &ca_engine::repository::RelativeSourcePath,
        ) -> Result<ExtractionFingerprints, ExtractionFailure> {
            Ok(ExtractionFingerprints {
                grammar: "slow-grammar".to_owned(),
                query: "slow-query".to_owned(),
                extractor: "slow-extractor".to_owned(),
            })
        }

        fn extractor_set_fingerprint(&self) -> String {
            "slow-extractor-set".to_owned()
        }
    }

    impl ExtractionWorker for SlowWorker {
        fn extract(
            &mut self,
            _source: &SourceFile,
            cancellation: &CancellationContext,
        ) -> Result<IndexedFile, ExtractionFailure> {
            self.started.store(true, Ordering::SeqCst);
            while !cancellation.is_cancelled() {
                thread::yield_now();
            }
            self.observed_cancel.store(true, Ordering::SeqCst);
            Err(ExtractionFailure {
                cancelled: true,
                message: "cancelled in worker".to_owned(),
            })
        }
    }

    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures")
    }

    fn open_test_storage(root_path: &Path, data_path: &Path) -> (RepositoryIdentity, Storage) {
        let root = ca_core::RepositoryRoot::new(root_path.to_owned()).expect("canonical root");
        let identity = RepositoryIdentity::derive(root).expect("derive identity");
        let paths = StoragePaths::under(data_path, identity.id(), identity.root().as_path())
            .expect("derive storage paths");
        let storage = Storage::open(identity.id().clone(), paths).expect("open storage");
        (identity, storage)
    }

    fn run_real_index(
        identity: RepositoryIdentity,
        storage: &Storage,
        mode: IndexMode,
    ) -> ca_engine::indexing::IndexOutcome {
        let adapter = StorageIndexAdapter::new(storage);
        let factory = ParserFactory;
        let service = IndexService::new(
            SourceReader::new(identity, ScanPolicy::default()),
            &adapter,
            &factory,
            IndexLimits {
                worker_count: 2,
                source_queue_capacity: 4,
                persist_batch_size: 8,
                max_warnings: 256,
            },
        )
        .expect("construct real index service");
        service
            .run(
                IndexRequest {
                    mode,
                    request_key: None,
                    config_fingerprint: DEFAULT_CONFIG_FINGERPRINT.to_owned(),
                    changed_paths: None,
                },
                &CancellationContext::default(),
            )
            .expect("index evaluation corpus")
    }

    fn run_targeted_index(
        identity: RepositoryIdentity,
        storage: &Storage,
        changed_paths: Vec<ca_engine::repository::RelativeSourcePath>,
    ) -> ca_engine::indexing::IndexOutcome {
        let adapter = StorageIndexAdapter::new(storage);
        let factory = ParserFactory;
        let service = IndexService::new(
            SourceReader::new(identity, ScanPolicy::default()),
            &adapter,
            &factory,
            IndexLimits::default(),
        )
        .expect("construct targeted index service");
        service
            .run(
                IndexRequest {
                    mode: IndexMode::Incremental,
                    request_key: None,
                    config_fingerprint: DEFAULT_CONFIG_FINGERPRINT.to_owned(),
                    changed_paths: Some(changed_paths),
                },
                &CancellationContext::default(),
            )
            .expect("run targeted index")
    }

    #[test]
    fn targeted_incremental_edit_clones_membership_and_prunes_deletions() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "codeatlas-targeted-index-{}-{nonce}",
            std::process::id()
        ));
        let root = base.join("root");
        let data = base.join("data");
        fs::create_dir_all(&root).expect("create source root");
        fs::create_dir_all(&data).expect("create data root");
        fs::write(root.join("a.rs"), "pub fn alpha() -> usize { 1 }\n").expect("write a");
        fs::write(root.join("b.rs"), "pub fn beta() -> usize { 2 }\n").expect("write b");
        let (identity, storage) = open_test_storage(&root, &data);
        run_real_index(identity.clone(), &storage, IndexMode::Full);

        fs::write(root.join("a.rs"), "pub fn alpha() -> usize { 3 }\n").expect("edit a");
        let edited = run_targeted_index(
            identity.clone(),
            &storage,
            vec![
                ca_engine::repository::RelativeSourcePath::new("a.rs").expect("valid source path"),
            ],
        );
        assert_eq!(edited.job.progress.files_discovered, 2);
        assert_eq!(edited.job.progress.files_reused, 1);
        assert_eq!(edited.job.progress.files_parsed, 1);
        assert_eq!(edited.job.progress.files_persisted, 2);
        assert_eq!(edited.job.progress.files_deleted, 0);
        assert_eq!(storage.active_files().expect("active files").len(), 2);

        fs::remove_file(root.join("b.rs")).expect("delete b");
        let deleted = run_targeted_index(
            identity,
            &storage,
            vec![
                ca_engine::repository::RelativeSourcePath::new("b.rs").expect("valid source path"),
            ],
        );
        assert_eq!(deleted.job.progress.files_discovered, 1);
        assert_eq!(deleted.job.progress.files_reused, 1);
        assert_eq!(deleted.job.progress.files_parsed, 0);
        assert_eq!(deleted.job.progress.files_persisted, 1);
        assert_eq!(deleted.job.progress.files_deleted, 1);
        let active = storage.active_files().expect("active after deletion");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].relative_path, "a.rs");

        drop(storage);
        fs::remove_dir_all(base).expect("remove targeted index fixture");
    }

    #[test]
    fn hand_labelled_resolution_corpus_matches_per_language_metrics() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let data_path = std::env::temp_dir().join(format!(
            "codeatlas-resolution-evaluation-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&data_path).expect("create evaluation data directory");
        let root_path = fixture_root();
        let (identity, storage) = open_test_storage(&root_path, &data_path);
        let outcome = run_real_index(identity, &storage, IndexMode::Full);
        assert_eq!(outcome.job.state, IndexJobState::Completed);
        let facts = storage
            .read_snapshot()
            .expect("read evaluation snapshot")
            .graph_facts(true, 10_000)
            .expect("read evaluation graph");
        let manifest: ResolutionManifest = serde_json::from_str(include_str!(
            "../../../fixtures/resolution-expectations.json"
        ))
        .expect("parse independent resolution labels");
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.corpus_root, "fixtures");

        let mut by_source = BTreeMap::<
            (String, String, String),
            BTreeMap<(u64, String), Vec<&ca_storage::StoredGraphFact>>,
        >::new();
        for fact in &facts {
            by_source
                .entry((
                    fact.source_path.clone(),
                    fact.source_category.clone(),
                    fact.source_spelling.clone(),
                ))
                .or_default()
                .entry((
                    fact.source_start_byte,
                    fact.edge.source_observation_id.clone(),
                ))
                .or_default()
                .push(fact);
        }

        let mut accumulators = BTreeMap::<String, MetricAccumulator>::new();
        for label in &manifest.labels {
            let accumulator = accumulators.entry(label.language.clone()).or_default();
            accumulator.sites = accumulator.sites.saturating_add(1);
            let sources = by_source
                .get(&(
                    label.source_path.clone(),
                    label.source_category.clone(),
                    label.source_spelling.clone(),
                ))
                .unwrap_or_else(|| panic!("missing labelled source: {label:?}"));
            let (_, actual) = sources
                .iter()
                .nth(label.occurrence)
                .unwrap_or_else(|| panic!("missing labelled occurrence: {label:?}"));
            accumulator.found = accumulator.found.saturating_add(1);
            assert!(
                actual
                    .iter()
                    .all(|fact| fact.edge.relationship == label.relationship),
                "relationship mismatch for {label:?}: {actual:?}"
            );
            assert!(
                actual
                    .iter()
                    .all(|fact| fact.edge.resolution == label.resolution),
                "resolution mismatch for {label:?}: {actual:?}"
            );
            assert!(
                actual
                    .iter()
                    .all(|fact| fact.candidate_count == label.candidate_count),
                "candidate-count mismatch for {label:?}: {actual:?}"
            );
            let mut actual_targets = actual
                .iter()
                .filter_map(|fact| {
                    fact.target_path.as_ref().map(|path| ExpectedTarget {
                        path: path.clone(),
                        name: fact.target_name.clone(),
                    })
                })
                .collect::<Vec<_>>();
            actual_targets.sort();
            let mut expected_targets = label.targets.clone();
            expected_targets.sort();
            assert_eq!(actual_targets, expected_targets, "targets for {label:?}");

            accumulator.predicted = accumulator
                .predicted
                .saturating_add(u64::try_from(actual_targets.len()).expect("target count"));
            accumulator.expected = accumulator
                .expected
                .saturating_add(u64::try_from(expected_targets.len()).expect("target count"));
            let mut unmatched = actual_targets;
            for expected in expected_targets {
                if let Some(index) = unmatched.iter().position(|actual| *actual == expected) {
                    unmatched.remove(index);
                    accumulator.correct = accumulator.correct.saturating_add(1);
                }
            }
            if label.resolution == "unresolved" {
                accumulator.unresolved = accumulator.unresolved.saturating_add(1);
            }
            if label.resolution == "candidate" {
                accumulator.candidates = accumulator.candidates.saturating_add(1);
            }
            if label.resolution == "syntax_observation" {
                accumulator.external = accumulator.external.saturating_add(1);
            }
        }

        let computed = accumulators
            .into_iter()
            .map(|(language, value)| LanguageMetrics {
                language,
                labelled_sites: value.sites,
                syntax_coverage: Fraction {
                    numerator: value.found,
                    denominator: value.sites,
                },
                binding_precision: Fraction {
                    numerator: value.correct,
                    denominator: value.predicted,
                },
                binding_recall: Fraction {
                    numerator: value.correct,
                    denominator: value.expected,
                },
                unresolved_rate: Fraction {
                    numerator: value.unresolved,
                    denominator: value.sites,
                },
                candidate_sites: value.candidates,
                external_observation_sites: value.external,
            })
            .collect::<Vec<_>>();
        let artifact: EvaluationArtifact = serde_json::from_str(include_str!(
            "../../../docs/reports/artifacts/09-resolution-evaluation.json"
        ))
        .expect("parse committed evaluation artifact");
        assert_eq!(artifact.schema_version, 1);
        assert_eq!(artifact.corpus, "fixtures/resolution-expectations.json");
        assert_eq!(computed, artifact.metrics);
        drop(storage);
        fs::remove_dir_all(data_path).expect("remove evaluation data");
    }

    fn copy_tree(source: &Path, target: &Path) {
        fs::create_dir_all(target).expect("create copied fixture directory");
        for entry in fs::read_dir(source).expect("read fixture directory") {
            let entry = entry.expect("fixture entry");
            let source_path = entry.path();
            let target_path = target.join(entry.file_name());
            if entry.file_type().expect("fixture type").is_dir() {
                copy_tree(&source_path, &target_path);
            } else {
                fs::copy(source_path, target_path).expect("copy fixture file");
            }
        }
    }

    fn canonical_graph(storage: &Storage) -> Vec<String> {
        let mut facts = storage
            .read_snapshot()
            .expect("read graph snapshot")
            .graph_facts(true, 10_000)
            .expect("read graph facts")
            .into_iter()
            .map(|fact| {
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    fact.source_path,
                    fact.source_category,
                    fact.source_spelling,
                    fact.source_start_byte,
                    fact.edge.relationship,
                    fact.edge.resolution,
                    fact.target_path.as_deref().unwrap_or(""),
                    fact.target_name.as_deref().unwrap_or(""),
                    fact.candidate_count,
                    fact.rule_version,
                )
            })
            .collect::<Vec<_>>();
        facts.sort();
        facts
    }

    #[test]
    fn changed_file_and_clean_full_index_converge_after_rename_delete_and_export_change() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "codeatlas-resolution-equivalence-{}-{nonce}",
            std::process::id()
        ));
        let incremental_root = base.join("incremental-root");
        let full_root = base.join("full-root");
        let initial = fixture_root().join("resolution-mutations/initial");
        let final_state = fixture_root().join("resolution-mutations/final");
        copy_tree(&initial, &incremental_root);
        copy_tree(&final_state, &full_root);
        let incremental_data = base.join("incremental-data");
        let full_data = base.join("full-data");
        fs::create_dir_all(&incremental_data).expect("create incremental data");
        fs::create_dir_all(&full_data).expect("create full data");

        let (identity, storage) = open_test_storage(&incremental_root, &incremental_data);
        run_real_index(identity.clone(), &storage, IndexMode::Full);
        fs::remove_file(incremental_root.join("provider.js")).expect("delete old provider");
        fs::remove_file(incremental_root.join("deleted.js")).expect("delete stale source");
        fs::copy(
            final_state.join("renamed.js"),
            incremental_root.join("renamed.js"),
        )
        .expect("copy renamed provider");
        fs::copy(
            final_state.join("consumer.js"),
            incremental_root.join("consumer.js"),
        )
        .expect("update consumer export use");
        let incremental = run_real_index(identity, &storage, IndexMode::Incremental);
        assert_eq!(incremental.job.progress.files_deleted, 2);
        assert_eq!(incremental.job.progress.files_parsed, 2);
        let incremental_graph = canonical_graph(&storage);
        assert!(
            incremental_graph
                .iter()
                .all(|edge| !edge.contains("provider.js") && !edge.contains("deleted.js"))
        );
        assert!(
            incremental_graph
                .iter()
                .any(|edge| edge.contains("renamed.js") && edge.contains("after"))
        );

        let (full_identity, full_storage) = open_test_storage(&full_root, &full_data);
        run_real_index(full_identity, &full_storage, IndexMode::Full);
        assert_eq!(incremental_graph, canonical_graph(&full_storage));
        drop(full_storage);
        drop(storage);
        fs::remove_dir_all(base).expect("remove equivalence layout");
    }

    #[test]
    fn durable_cancellation_reaches_an_active_worker_and_preserves_no_generation() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("codeatlas-cancel-{}-{nonce}", std::process::id()));
        let root_path = base.join("repo");
        let data_path = base.join("data");
        fs::create_dir_all(&root_path).expect("create repository");
        fs::create_dir_all(&data_path).expect("create data base");
        fs::write(root_path.join("slow.rs"), "pub fn slow() {}\n").expect("write source");
        let root = ca_core::RepositoryRoot::new(root_path).expect("canonical root");
        let identity = RepositoryIdentity::derive(root).expect("derive identity");
        let paths = StoragePaths::under(&data_path, identity.id(), identity.root().as_path())
            .expect("derive storage paths");
        let storage = Storage::open(identity.id().clone(), paths).expect("open storage");
        let adapter = StorageIndexAdapter::new(&storage);
        let started = Arc::new(AtomicBool::new(false));
        let observed_cancel = Arc::new(AtomicBool::new(false));
        let factory = SlowFactory {
            started: Arc::clone(&started),
            observed_cancel: Arc::clone(&observed_cancel),
        };
        let service = IndexService::new(
            SourceReader::new(identity, ScanPolicy::default()),
            &adapter,
            &factory,
            IndexLimits {
                worker_count: 1,
                source_queue_capacity: 1,
                persist_batch_size: 1,
                max_warnings: 8,
            },
        )
        .expect("construct index service");
        let cancellation = CancellationContext::default();

        thread::scope(|scope| {
            let running = scope.spawn(|| {
                service.run(
                    IndexRequest {
                        mode: IndexMode::Full,
                        request_key: None,
                        config_fingerprint: "cancel-test-config".to_owned(),
                        changed_paths: None,
                    },
                    &cancellation,
                )
            });
            let deadline = Instant::now() + Duration::from_secs(5);
            let job = loop {
                if let Some(job) = storage.index_summary().expect("read job status").latest_job
                    && job.state == JobState::Parsing
                    && started.load(Ordering::SeqCst)
                {
                    break job;
                }
                if Instant::now() >= deadline {
                    cancellation.cancel();
                    panic!("worker did not start before deadline");
                }
                thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(
                storage
                    .request_cancel(job.id)
                    .expect("request durable cancellation"),
                CancelJobResult::Requested
            );
            assert!(matches!(
                running.join().expect("index thread did not panic"),
                Err(IndexingError::Cancelled)
            ));
        });

        assert!(observed_cancel.load(Ordering::SeqCst));
        let summary = storage.index_summary().expect("read final index status");
        assert!(summary.active_generation_id.is_none());
        assert_eq!(
            summary.latest_job.expect("cancelled job").state,
            JobState::Cancelled
        );
        drop(storage);
        let _ = fs::remove_dir_all(base);
    }
}
