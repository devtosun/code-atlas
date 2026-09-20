use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    error::Error,
    time::{Duration, Instant},
};

use ca_core::{GenerationId, RepositoryId};
use serde::Serialize;
use thiserror::Error;

use crate::repository::{RelativeSourcePath, RepositoryError, SourceFile, SourceReader};

pub const DEFAULT_RESPONSE_BYTES: usize = 65_536;
pub const MAX_QUERY_BYTES: usize = 256;
pub const MAX_RESULT_LIMIT: usize = 200;
const MAX_CURSOR_BYTES: usize = 4_096;
const CURSOR_VERSION: &str = "ca1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalLimits {
    pub max_query_bytes: usize,
    pub max_results: usize,
    pub max_outline_symbols: usize,
    pub max_repo_entries: usize,
    pub max_snippet_bytes: usize,
    pub max_read_lines: usize,
    pub max_graph_depth: usize,
    pub max_graph_nodes: usize,
    pub max_graph_edges: usize,
    pub max_deadline: Duration,
    pub max_response_bytes: usize,
}

impl Default for RetrievalLimits {
    fn default() -> Self {
        Self {
            max_query_bytes: MAX_QUERY_BYTES,
            max_results: MAX_RESULT_LIMIT,
            max_outline_symbols: 2_000,
            max_repo_entries: 5_000,
            max_snippet_bytes: 8 * 1_024,
            max_read_lines: 400,
            max_graph_depth: 8,
            max_graph_nodes: 500,
            max_graph_edges: 2_000,
            max_deadline: Duration::from_secs(5),
            max_response_bytes: DEFAULT_RESPONSE_BYTES,
        }
    }
}

impl RetrievalLimits {
    pub fn validate(self) -> Result<Self, RetrievalError> {
        if self.max_query_bytes == 0
            || self.max_query_bytes > 4_096
            || self.max_results == 0
            || self.max_results > 1_000
            || self.max_outline_symbols == 0
            || self.max_outline_symbols > 20_000
            || self.max_repo_entries == 0
            || self.max_repo_entries > 50_000
            || self.max_snippet_bytes == 0
            || self.max_snippet_bytes > 64 * 1_024
            || self.max_read_lines == 0
            || self.max_read_lines > 10_000
            || self.max_graph_depth == 0
            || self.max_graph_depth > 8
            || self.max_graph_nodes == 0
            || self.max_graph_nodes > 10_000
            || self.max_graph_edges == 0
            || self.max_graph_edges > 100_000
            || self.max_deadline.is_zero()
            || self.max_deadline > Duration::from_secs(30)
            || self.max_response_bytes < 512
            || self.max_response_bytes > DEFAULT_RESPONSE_BYTES
        {
            return Err(RetrievalError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Error)]
pub enum RetrievalError {
    #[error("invalid retrieval limits")]
    InvalidLimits,
    #[error("query must contain 1..={max} UTF-8 bytes and no NUL")]
    InvalidQuery { max: usize },
    #[error("result limit is outside policy")]
    InvalidResultLimit,
    #[error("response byte budget is outside policy")]
    InvalidResponseBudget,
    #[error("malformed cursor")]
    InvalidCursor,
    #[error("cursor belongs to a different repository")]
    ForeignCursor,
    #[error("cursor generation is no longer the selected snapshot")]
    StaleCursor,
    #[error("cursor does not match this query and filters")]
    CursorQueryMismatch,
    #[error("symbol was not found in the selected generation: {0}")]
    SymbolNotFound(String),
    #[error("path is not indexed in the selected generation: {0}")]
    FileNotIndexed(String),
    #[error("indexed source content changed; reindex required: {0}")]
    ContentChanged(String),
    #[error("indexed source was deleted: {0}")]
    SourceDeleted(String),
    #[error("source is excluded by the current repository policy: {0}")]
    SourceExcluded(String),
    #[error("source access failed for '{path}': {reason}")]
    SourceAccess { path: String, reason: String },
    #[error("line or byte range is outside policy")]
    InvalidRange,
    #[error("graph traversal exceeded its deadline")]
    DeadlineExceeded,
    #[error("retrieval store operation failed: {0}")]
    Store(String),
    #[error("result cannot fit the requested serialized byte budget")]
    ResponseTooLarge,
    #[error("cannot serialize retrieval result: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CoverageSummary {
    pub status: String,
    pub warning_count: u64,
    pub unresolved_occurrences: u64,
    pub resolver_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SymbolRecord {
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
    pub attributes: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchRow {
    pub symbol: SymbolRecord,
    pub tier: u8,
    pub fts_score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchSortKey {
    pub tier: u8,
    pub fts_score: f64,
    pub relative_path: String,
    pub name: String,
    pub start_byte: u64,
    pub symbol_id: String,
}

impl SearchSortKey {
    #[must_use]
    pub fn from_row(row: &SearchRow) -> Self {
        Self {
            tier: row.tier,
            fts_score: row.fts_score,
            relative_path: row.symbol.relative_path.clone(),
            name: row.symbol.name.clone(),
            start_byte: row.symbol.start_byte,
            symbol_id: row.symbol.id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchPlan {
    pub query: String,
    pub folded_query: String,
    pub escaped_folded_prefix: String,
    pub fts_expression: String,
    pub language: Option<String>,
    pub kind: Option<String>,
    pub path_prefix: Option<String>,
    pub after: Option<SearchSortKey>,
    pub fetch_limit: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationFilter {
    Any,
    Calls,
    References,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RelationRecord {
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
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReferenceRecord {
    pub edge_id: String,
    pub source_observation_id: String,
    pub relative_path: String,
    pub spelling: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub resolution: String,
    pub candidate_count: u64,
    pub rule_version: String,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReferenceSortKey {
    pub relative_path: String,
    pub start_byte: u64,
    pub edge_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IndexedFileRecord {
    pub relative_path: String,
    pub content_hash: String,
    pub language: String,
    pub parse_status: String,
    pub coverage: String,
    pub byte_length: u64,
    pub symbol_count: u64,
}

pub trait RetrievalStore {
    type Error: Error + Send + Sync + 'static;

    fn repository_id(&self) -> &RepositoryId;
    fn generation_id(&self) -> &GenerationId;
    fn coverage(&self) -> Result<CoverageSummary, Self::Error>;
    fn search_symbols(&self, plan: &SearchPlan) -> Result<Vec<SearchRow>, Self::Error>;
    fn symbol(&self, symbol_id: &str) -> Result<Option<SymbolRecord>, Self::Error>;
    fn file_symbols(
        &self,
        relative_path: &str,
        limit: usize,
    ) -> Result<Vec<SymbolRecord>, Self::Error>;
    fn references_to(
        &self,
        symbol_id: &str,
        include_candidates: bool,
        after: Option<&ReferenceSortKey>,
        limit: usize,
    ) -> Result<Vec<ReferenceRecord>, Self::Error>;
    fn adjacent_relations(
        &self,
        symbol_id: &str,
        direction: RelationDirection,
        filter: RelationFilter,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<RelationRecord>, Self::Error>;
    fn incoming_relations_to_path(
        &self,
        relative_path: &str,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<RelationRecord>, Self::Error>;
    fn repository_files(
        &self,
        path_prefix: Option<&str>,
        limit: usize,
    ) -> Result<Vec<IndexedFileRecord>, Self::Error>;
    fn indexed_file(&self, relative_path: &str) -> Result<Option<IndexedFileRecord>, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SearchFilter {
    pub language: Option<String>,
    pub kind: Option<String>,
    pub path_prefix: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchRequest {
    pub query: String,
    pub filter: SearchFilter,
    pub limit: usize,
    pub cursor: Option<String>,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchHit {
    pub symbol: SymbolRecord,
    pub match_kind: String,
    pub fts_score: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchPage {
    pub repository_id: String,
    pub generation_id: String,
    pub query: String,
    pub results: Vec<SearchHit>,
    pub coverage: CoverageSummary,
    pub truncated: bool,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SymbolResult {
    pub repository_id: String,
    pub generation_id: String,
    pub symbol: SymbolRecord,
    pub coverage: CoverageSummary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlineRequest {
    pub relative_path: String,
    pub limit: usize,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileOutline {
    pub repository_id: String,
    pub generation_id: String,
    pub file: IndexedFileRecord,
    pub symbols: Vec<SymbolRecord>,
    pub coverage: CoverageSummary,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferencesRequest {
    pub symbol_id: String,
    pub include_candidates: bool,
    pub limit: usize,
    pub cursor: Option<String>,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReferencesPage {
    pub repository_id: String,
    pub generation_id: String,
    pub symbol_id: String,
    pub references: Vec<ReferenceRecord>,
    pub coverage: CoverageSummary,
    pub include_candidates: bool,
    pub truncated: bool,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceRequest {
    pub symbol_id: String,
    pub direction: RelationDirection,
    pub depth: usize,
    pub include_candidates: bool,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub deadline: Duration,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TraceResult {
    pub repository_id: String,
    pub generation_id: String,
    pub start_symbol_id: String,
    pub direction: RelationDirection,
    pub include_candidates: bool,
    pub visited_symbol_ids: Vec<String>,
    pub edges: Vec<RelationRecord>,
    pub coverage: CoverageSummary,
    pub warnings: Vec<String>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoMapRequest {
    pub scope: Option<String>,
    pub depth: usize,
    pub limit: usize,
    pub max_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoMapEntryKind {
    Directory,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepoMapEntry {
    pub path: String,
    pub kind: RepoMapEntryKind,
    pub language: Option<String>,
    pub parse_status: Option<String>,
    pub symbol_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepoMap {
    pub repository_id: String,
    pub generation_id: String,
    pub scope: Option<String>,
    pub entries: Vec<RepoMapEntry>,
    pub coverage: CoverageSummary,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactRequest {
    pub symbol_ids: Vec<String>,
    pub changed_paths: Vec<String>,
    pub depth: usize,
    pub include_candidates: bool,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub deadline: Duration,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ImpactResult {
    pub repository_id: String,
    pub generation_id: String,
    pub root_symbol_ids: Vec<String>,
    pub changed_paths: Vec<String>,
    pub impacted_symbol_ids: Vec<String>,
    pub impacted_paths: Vec<String>,
    pub edges: Vec<RelationRecord>,
    pub coverage: CoverageSummary,
    pub warnings: Vec<String>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadCodeRequest {
    pub relative_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub expected_hash: String,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CodeExcerpt {
    pub repository_id: String,
    pub generation_id: String,
    pub relative_path: String,
    pub content_hash: String,
    pub start_line: usize,
    pub end_line: usize,
    pub code: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextRequest {
    pub query: String,
    pub scope: Option<String>,
    pub include_candidates: bool,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContextSymbol {
    pub symbol: SymbolRecord,
    pub match_kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceSnippet {
    pub relative_path: String,
    pub content_hash: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub start_line: usize,
    pub end_line: usize,
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContextBundle {
    pub repository_id: String,
    pub generation_id: String,
    pub query: String,
    pub symbols: Vec<ContextSymbol>,
    pub snippets: Vec<EvidenceSnippet>,
    pub relations: Vec<RelationRecord>,
    pub coverage: CoverageSummary,
    pub warnings: Vec<String>,
    pub truncated: bool,
    pub approximate_tokens: usize,
    pub token_estimate_method: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Budgeted<T> {
    pub data: T,
    pub structured_json: String,
    pub text_fallback: String,
    pub serialized_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CursorPayload {
    operation: String,
    repository_id: String,
    generation_id: String,
    query_fingerprint: String,
    sort: Vec<String>,
}

pub struct RetrievalService<'a, S> {
    store: &'a S,
    reader: &'a SourceReader,
    limits: RetrievalLimits,
}

impl<'a, S> RetrievalService<'a, S>
where
    S: RetrievalStore,
{
    pub fn new(
        store: &'a S,
        reader: &'a SourceReader,
        limits: RetrievalLimits,
    ) -> Result<Self, RetrievalError> {
        Ok(Self {
            store,
            reader,
            limits: limits.validate()?,
        })
    }

    pub fn search_symbols(
        &self,
        request: SearchRequest,
    ) -> Result<Budgeted<SearchPage>, RetrievalError> {
        let query = self.validate_query(&request.query)?;
        self.validate_limit(request.limit, self.limits.max_results)?;
        self.validate_response_budget(request.max_bytes)?;
        validate_filter_value(request.filter.language.as_deref())?;
        validate_filter_value(request.filter.kind.as_deref())?;
        let path_prefix = validate_optional_path_prefix(request.filter.path_prefix.as_deref())?;
        let fingerprint = fingerprint(&[
            "search_symbols",
            &query,
            request.filter.language.as_deref().unwrap_or(""),
            request.filter.kind.as_deref().unwrap_or(""),
            path_prefix.as_deref().unwrap_or(""),
        ]);
        let after = request
            .cursor
            .as_deref()
            .map(|cursor| self.decode_search_cursor(cursor, &fingerprint))
            .transpose()?;
        let tokens = identifier_tokens(&query);
        let plan = SearchPlan {
            folded_query: query.to_lowercase(),
            escaped_folded_prefix: escape_like_prefix(&query.to_lowercase()),
            fts_expression: literal_fts_expression(&tokens),
            query: query.clone(),
            language: request.filter.language,
            kind: request.filter.kind,
            path_prefix,
            after,
            fetch_limit: request.limit.saturating_add(1),
        };
        let mut rows = self
            .store
            .search_symbols(&plan)
            .map_err(store_error::<S::Error>)?;
        rows.sort_by(compare_search_rows);
        rows.truncate(plan.fetch_limit);
        let has_more = rows.len() > request.limit;
        rows.truncate(request.limit);
        let mut page = SearchPage {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            query,
            results: rows.iter().map(search_hit).collect(),
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
            truncated: has_more,
            next_cursor: has_more
                .then(|| rows.last())
                .flatten()
                .map(|row| self.encode_search_cursor(&fingerprint, &SearchSortKey::from_row(row))),
        };
        fit_budget(&mut page, request.max_bytes, |page| {
            if page.results.pop().is_none() {
                return false;
            }
            page.truncated = true;
            page.next_cursor = page.results.last().map(|hit| {
                let row = SearchRow {
                    symbol: hit.symbol.clone(),
                    tier: match_kind_tier(&hit.match_kind),
                    fts_score: hit.fts_score.unwrap_or(0.0),
                };
                self.encode_search_cursor(&fingerprint, &SearchSortKey::from_row(&row))
            });
            true
        })
    }

    pub fn get_symbol(
        &self,
        symbol_id: &str,
        max_bytes: usize,
    ) -> Result<Budgeted<SymbolResult>, RetrievalError> {
        validate_identifier(symbol_id)?;
        self.validate_response_budget(max_bytes)?;
        let symbol = self
            .store
            .symbol(symbol_id)
            .map_err(store_error::<S::Error>)?
            .ok_or_else(|| RetrievalError::SymbolNotFound(symbol_id.to_owned()))?;
        let mut result = SymbolResult {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            symbol,
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
        };
        fit_budget(&mut result, max_bytes, |_| false)
    }

    pub fn file_outline(
        &self,
        request: OutlineRequest,
    ) -> Result<Budgeted<FileOutline>, RetrievalError> {
        self.validate_limit(request.limit, self.limits.max_outline_symbols)?;
        self.validate_response_budget(request.max_bytes)?;
        let path = RelativeSourcePath::new(request.relative_path.clone())
            .map_err(|_| RetrievalError::InvalidQuery { max: 4_096 })?;
        let file = self
            .store
            .indexed_file(path.as_str())
            .map_err(store_error::<S::Error>)?
            .ok_or_else(|| RetrievalError::FileNotIndexed(path.as_str().to_owned()))?;
        let mut symbols = self
            .store
            .file_symbols(path.as_str(), request.limit.saturating_add(1))
            .map_err(store_error::<S::Error>)?;
        let truncated = symbols.len() > request.limit;
        symbols.truncate(request.limit);
        let mut outline = FileOutline {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            file,
            symbols,
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
            truncated,
        };
        fit_budget(&mut outline, request.max_bytes, |outline| {
            if outline.symbols.pop().is_none() {
                return false;
            }
            outline.truncated = true;
            true
        })
    }

    pub fn find_references(
        &self,
        request: ReferencesRequest,
    ) -> Result<Budgeted<ReferencesPage>, RetrievalError> {
        validate_identifier(&request.symbol_id)?;
        self.validate_limit(request.limit, self.limits.max_results)?;
        self.validate_response_budget(request.max_bytes)?;
        if self
            .store
            .symbol(&request.symbol_id)
            .map_err(store_error::<S::Error>)?
            .is_none()
        {
            return Err(RetrievalError::SymbolNotFound(request.symbol_id));
        }
        let fingerprint = fingerprint(&[
            "find_references",
            &request.symbol_id,
            if request.include_candidates { "1" } else { "0" },
        ]);
        let after = request
            .cursor
            .as_deref()
            .map(|cursor| self.decode_reference_cursor(cursor, &fingerprint))
            .transpose()?;
        let mut references = self
            .store
            .references_to(
                &request.symbol_id,
                request.include_candidates,
                after.as_ref(),
                request.limit.saturating_add(1),
            )
            .map_err(store_error::<S::Error>)?;
        references.sort_by_key(reference_sort_key);
        let has_more = references.len() > request.limit;
        references.truncate(request.limit);
        let mut page = ReferencesPage {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            symbol_id: request.symbol_id,
            references,
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
            include_candidates: request.include_candidates,
            truncated: has_more,
            next_cursor: None,
        };
        if has_more {
            page.next_cursor = page.references.last().map(|reference| {
                self.encode_reference_cursor(&fingerprint, &reference_sort_key(reference))
            });
        }
        fit_budget(&mut page, request.max_bytes, |page| {
            if page.references.pop().is_none() {
                return false;
            }
            page.truncated = true;
            page.next_cursor = page.references.last().map(|reference| {
                self.encode_reference_cursor(&fingerprint, &reference_sort_key(reference))
            });
            true
        })
    }

    pub fn trace_calls(
        &self,
        request: TraceRequest,
    ) -> Result<Budgeted<TraceResult>, RetrievalError> {
        validate_identifier(&request.symbol_id)?;
        self.validate_graph_request(
            request.depth,
            request.max_nodes,
            request.max_edges,
            request.deadline,
        )?;
        self.validate_response_budget(request.max_bytes)?;
        if self
            .store
            .symbol(&request.symbol_id)
            .map_err(store_error::<S::Error>)?
            .is_none()
        {
            return Err(RetrievalError::SymbolNotFound(request.symbol_id));
        }
        let (visited_symbol_ids, edges, traversal_truncated) = self.walk_graph(
            std::slice::from_ref(&request.symbol_id),
            request.direction,
            RelationFilter::Calls,
            request.include_candidates,
            request.depth,
            request.max_nodes,
            request.max_edges,
            request.deadline,
        )?;
        let mut warnings = syntax_graph_warnings();
        if request.direction == RelationDirection::Incoming && edges.is_empty() {
            warnings.push(
                "No observed callers is not proof that the symbol is unused or dead code."
                    .to_owned(),
            );
        }
        let mut result = TraceResult {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            start_symbol_id: request.symbol_id,
            direction: request.direction,
            include_candidates: request.include_candidates,
            visited_symbol_ids,
            edges,
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
            warnings,
            truncated: traversal_truncated,
        };
        fit_budget(&mut result, request.max_bytes, |result| {
            if result.edges.pop().is_some() {
                result.truncated = true;
                return true;
            }
            if result.visited_symbol_ids.len() > 1 {
                result.visited_symbol_ids.pop();
                result.truncated = true;
                return true;
            }
            false
        })
    }

    pub fn repo_map(&self, request: RepoMapRequest) -> Result<Budgeted<RepoMap>, RetrievalError> {
        self.validate_limit(request.limit, self.limits.max_repo_entries)?;
        self.validate_response_budget(request.max_bytes)?;
        if request.depth > self.limits.max_graph_depth {
            return Err(RetrievalError::InvalidLimits);
        }
        let scope = validate_optional_path_prefix(request.scope.as_deref())?;
        let mut files = self
            .store
            .repository_files(scope.as_deref(), request.limit.saturating_add(1))
            .map_err(store_error::<S::Error>)?;
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let mut truncated = files.len() > request.limit;
        files.truncate(request.limit);
        let mut directories = BTreeMap::<String, u64>::new();
        for file in &files {
            let mut parent = String::new();
            let segments = file.relative_path.split('/').collect::<Vec<_>>();
            for segment in segments.iter().take(segments.len().saturating_sub(1)) {
                if !parent.is_empty() {
                    parent.push('/');
                }
                parent.push_str(segment);
                *directories.entry(parent.clone()).or_default() += file.symbol_count;
            }
        }
        let scope_segments = scope
            .as_deref()
            .map(|value| value.split('/').count())
            .unwrap_or(0);
        let within_depth = |path: &str| {
            path.split('/').count().saturating_sub(scope_segments)
                <= request.depth.saturating_add(1)
        };
        let mut entries = directories
            .into_iter()
            .filter(|(path, _)| within_depth(path))
            .map(|(path, symbol_count)| RepoMapEntry {
                path,
                kind: RepoMapEntryKind::Directory,
                language: None,
                parse_status: None,
                symbol_count,
            })
            .chain(
                files
                    .into_iter()
                    .filter(|file| within_depth(&file.relative_path))
                    .map(|file| RepoMapEntry {
                        path: file.relative_path,
                        kind: RepoMapEntryKind::File,
                        language: Some(file.language),
                        parse_status: Some(file.parse_status),
                        symbol_count: file.symbol_count,
                    }),
            )
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| repo_entry_rank(left.kind).cmp(&repo_entry_rank(right.kind)))
        });
        if entries.len() > request.limit {
            entries.truncate(request.limit);
            truncated = true;
        }
        let mut map = RepoMap {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            scope,
            entries,
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
            truncated,
        };
        fit_budget(&mut map, request.max_bytes, |map| {
            if map.entries.pop().is_none() {
                return false;
            }
            map.truncated = true;
            true
        })
    }

    pub fn analyze_impact(
        &self,
        request: ImpactRequest,
    ) -> Result<Budgeted<ImpactResult>, RetrievalError> {
        self.validate_graph_request(
            request.depth,
            request.max_nodes,
            request.max_edges,
            request.deadline,
        )?;
        self.validate_response_budget(request.max_bytes)?;
        if request.symbol_ids.is_empty() && request.changed_paths.is_empty() {
            return Err(RetrievalError::InvalidQuery { max: 4_096 });
        }
        let started = Instant::now();
        let mut roots = BTreeSet::new();
        let mut seed_edges = Vec::new();
        for symbol_id in &request.symbol_ids {
            validate_identifier(symbol_id)?;
            if self
                .store
                .symbol(symbol_id)
                .map_err(store_error::<S::Error>)?
                .is_none()
            {
                return Err(RetrievalError::SymbolNotFound(symbol_id.clone()));
            }
            roots.insert(symbol_id.clone());
        }
        let mut changed_paths = BTreeSet::new();
        for path in &request.changed_paths {
            self.ensure_deadline(started, request.deadline)?;
            let path = RelativeSourcePath::new(path.clone())
                .map_err(|_| RetrievalError::InvalidQuery { max: 4_096 })?;
            if self
                .store
                .indexed_file(path.as_str())
                .map_err(store_error::<S::Error>)?
                .is_none()
            {
                return Err(RetrievalError::FileNotIndexed(path.as_str().to_owned()));
            }
            changed_paths.insert(path.as_str().to_owned());
            for symbol in self
                .store
                .file_symbols(path.as_str(), request.max_nodes.saturating_add(1))
                .map_err(store_error::<S::Error>)?
            {
                if roots.len() < request.max_nodes {
                    roots.insert(symbol.id);
                }
            }
            let mut incoming = self
                .store
                .incoming_relations_to_path(
                    path.as_str(),
                    request.include_candidates,
                    request.max_edges.saturating_add(1),
                )
                .map_err(store_error::<S::Error>)?;
            incoming.sort_by(relation_order);
            for edge in incoming {
                if let Some(source) = &edge.source_symbol_id
                    && roots.len() < request.max_nodes
                {
                    roots.insert(source.clone());
                }
                if seed_edges.len() < request.max_edges {
                    seed_edges.push(edge);
                }
            }
        }
        let root_symbol_ids = roots.into_iter().collect::<Vec<_>>();
        let remaining_edges = request.max_edges.saturating_sub(seed_edges.len()).max(1);
        let remaining_deadline = request
            .deadline
            .checked_sub(started.elapsed())
            .ok_or(RetrievalError::DeadlineExceeded)?;
        let (mut impacted_symbol_ids, mut edges, mut truncated) = self.walk_graph(
            &root_symbol_ids,
            RelationDirection::Incoming,
            RelationFilter::Any,
            request.include_candidates,
            request.depth,
            request.max_nodes,
            remaining_edges,
            remaining_deadline,
        )?;
        if seed_edges.len() >= request.max_edges {
            truncated = true;
        }
        seed_edges.append(&mut edges);
        seed_edges.sort_by(relation_order);
        seed_edges.dedup_by(|left, right| left.id == right.id);
        if seed_edges.len() > request.max_edges {
            seed_edges.truncate(request.max_edges);
            truncated = true;
        }
        impacted_symbol_ids.sort();
        impacted_symbol_ids.dedup();
        let mut impacted_paths = BTreeSet::new();
        for symbol_id in &impacted_symbol_ids {
            if let Some(symbol) = self
                .store
                .symbol(symbol_id)
                .map_err(store_error::<S::Error>)?
            {
                impacted_paths.insert(symbol.relative_path);
            }
        }
        for edge in &seed_edges {
            impacted_paths.insert(edge.source_path.clone());
            if let Some(path) = &edge.target_path {
                impacted_paths.insert(path.clone());
            }
        }
        let mut result = ImpactResult {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            root_symbol_ids,
            changed_paths: changed_paths.into_iter().collect(),
            impacted_symbol_ids,
            impacted_paths: impacted_paths.into_iter().collect(),
            edges: seed_edges,
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
            warnings: syntax_graph_warnings(),
            truncated,
        };
        fit_budget(&mut result, request.max_bytes, |result| {
            if result.edges.pop().is_some() {
                result.truncated = true;
                return true;
            }
            if result.impacted_symbol_ids.pop().is_some() {
                result.truncated = true;
                return true;
            }
            if result.impacted_paths.pop().is_some() {
                result.truncated = true;
                return true;
            }
            false
        })
    }

    pub fn read_code(
        &self,
        request: ReadCodeRequest,
    ) -> Result<Budgeted<CodeExcerpt>, RetrievalError> {
        self.validate_response_budget(request.max_bytes)?;
        if request.start_line == 0
            || request.end_line < request.start_line
            || request.end_line - request.start_line + 1 > self.limits.max_read_lines
        {
            return Err(RetrievalError::InvalidRange);
        }
        let path = RelativeSourcePath::new(request.relative_path.clone())
            .map_err(|_| RetrievalError::InvalidRange)?;
        let indexed = self
            .store
            .indexed_file(path.as_str())
            .map_err(store_error::<S::Error>)?
            .ok_or_else(|| RetrievalError::FileNotIndexed(path.as_str().to_owned()))?;
        if indexed.content_hash != request.expected_hash {
            return Err(RetrievalError::ContentChanged(path.as_str().to_owned()));
        }
        let source = self.fresh_source(&path, &indexed.content_hash)?;
        let (code, actual_end) = line_excerpt(
            source.text(),
            request.start_line,
            request.end_line,
            self.limits.max_snippet_bytes,
        )?;
        let mut excerpt = CodeExcerpt {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            relative_path: path.as_str().to_owned(),
            content_hash: source.content_hash().to_owned(),
            start_line: request.start_line,
            end_line: actual_end,
            truncated: actual_end < request.end_line,
            code,
        };
        fit_budget(&mut excerpt, request.max_bytes, |excerpt| {
            let Some(position) = excerpt.code.trim_end_matches('\n').rfind('\n') else {
                return false;
            };
            excerpt.code.truncate(position + 1);
            excerpt.end_line = excerpt.end_line.saturating_sub(1);
            excerpt.truncated = true;
            true
        })
    }

    pub fn build_context(
        &self,
        request: ContextRequest,
    ) -> Result<Budgeted<ContextBundle>, RetrievalError> {
        let query = self.validate_query(&request.query)?;
        self.validate_response_budget(request.max_bytes)?;
        let scope = validate_optional_path_prefix(request.scope.as_deref())?;
        let tokens = identifier_tokens(&query);
        let plan = SearchPlan {
            query: query.clone(),
            folded_query: query.to_lowercase(),
            escaped_folded_prefix: escape_like_prefix(&query.to_lowercase()),
            fts_expression: literal_fts_expression(&tokens),
            language: None,
            kind: None,
            path_prefix: scope,
            after: None,
            fetch_limit: self.limits.max_results.min(24),
        };
        let mut rows = self
            .store
            .search_symbols(&plan)
            .map_err(store_error::<S::Error>)?;
        rows.sort_by(compare_search_rows);
        rows.truncate(plan.fetch_limit);
        let symbols = rows
            .iter()
            .map(|row| ContextSymbol {
                symbol: row.symbol.clone(),
                match_kind: search_hit(row).match_kind,
            })
            .collect::<Vec<_>>();
        let mut snippets = Vec::new();
        let mut warnings = syntax_graph_warnings();
        let mut seen_ranges = BTreeMap::<String, Vec<(u64, u64)>>::new();
        for row in &rows {
            let path = RelativeSourcePath::new(row.symbol.relative_path.clone())
                .map_err(|_| RetrievalError::InvalidRange)?;
            let indexed = self
                .store
                .indexed_file(path.as_str())
                .map_err(store_error::<S::Error>)?
                .ok_or_else(|| RetrievalError::FileNotIndexed(path.as_str().to_owned()))?;
            match self.fresh_source(&path, &indexed.content_hash) {
                Ok(source) => {
                    let start = row.symbol.syntax_start_byte.min(indexed.byte_length);
                    let end = row.symbol.syntax_end_byte.min(indexed.byte_length);
                    if start >= end
                        || seen_ranges
                            .entry(path.as_str().to_owned())
                            .or_default()
                            .iter()
                            .any(|range| ranges_overlap(*range, (start, end)))
                    {
                        continue;
                    }
                    let snippet = byte_excerpt(&source, start, end, self.limits.max_snippet_bytes)?;
                    seen_ranges
                        .entry(path.as_str().to_owned())
                        .or_default()
                        .push((snippet.start_byte, snippet.end_byte));
                    snippets.push(snippet);
                }
                Err(error) => warnings.push(error.to_string()),
            }
        }
        let mut relations = Vec::new();
        for row in rows.iter().take(12) {
            for direction in [RelationDirection::Outgoing, RelationDirection::Incoming] {
                let mut adjacent = self
                    .store
                    .adjacent_relations(
                        &row.symbol.id,
                        direction,
                        RelationFilter::Any,
                        request.include_candidates,
                        16,
                    )
                    .map_err(store_error::<S::Error>)?;
                relations.append(&mut adjacent);
            }
        }
        relations.sort_by(relation_order);
        relations.dedup_by(|left, right| left.id == right.id);
        let mut bundle = ContextBundle {
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            query,
            symbols,
            snippets,
            relations,
            coverage: self.store.coverage().map_err(store_error::<S::Error>)?,
            warnings,
            truncated: false,
            approximate_tokens: 0,
            token_estimate_method: "ceil(UTF-8 JSON bytes / 4)".to_owned(),
        };
        refresh_token_estimate(&mut bundle)?;
        fit_budget(&mut bundle, request.max_bytes, |bundle| {
            let reduced = bundle.relations.pop().is_some()
                || bundle.snippets.pop().is_some()
                || bundle.symbols.pop().is_some();
            if reduced {
                bundle.truncated = true;
                let _ = refresh_token_estimate(bundle);
            }
            reduced
        })
    }

    fn validate_graph_request(
        &self,
        depth: usize,
        max_nodes: usize,
        max_edges: usize,
        deadline: Duration,
    ) -> Result<(), RetrievalError> {
        if depth > self.limits.max_graph_depth
            || !(1..=self.limits.max_graph_nodes).contains(&max_nodes)
            || !(1..=self.limits.max_graph_edges).contains(&max_edges)
            || deadline.is_zero()
            || deadline > self.limits.max_deadline
        {
            return Err(RetrievalError::InvalidLimits);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_graph(
        &self,
        roots: &[String],
        direction: RelationDirection,
        filter: RelationFilter,
        include_candidates: bool,
        depth: usize,
        max_nodes: usize,
        max_edges: usize,
        deadline: Duration,
    ) -> Result<(Vec<String>, Vec<RelationRecord>, bool), RetrievalError> {
        let started = Instant::now();
        let mut queue = VecDeque::new();
        let mut visited = BTreeSet::new();
        for root in roots.iter().take(max_nodes) {
            if visited.insert(root.clone()) {
                queue.push_back((root.clone(), 0_usize));
            }
        }
        let mut edges = Vec::new();
        let mut edge_ids = HashSet::new();
        let mut truncated = roots.len() > max_nodes;
        while let Some((symbol_id, current_depth)) = queue.pop_front() {
            self.ensure_deadline(started, deadline)?;
            if current_depth >= depth {
                continue;
            }
            let remaining = max_edges.saturating_sub(edges.len());
            if remaining == 0 {
                truncated = true;
                break;
            }
            let mut adjacent = self
                .store
                .adjacent_relations(
                    &symbol_id,
                    direction,
                    filter,
                    include_candidates,
                    remaining.saturating_add(1),
                )
                .map_err(store_error::<S::Error>)?;
            adjacent.sort_by(relation_order);
            if adjacent.len() > remaining {
                adjacent.truncate(remaining);
                truncated = true;
            }
            for edge in adjacent {
                if edge_ids.insert(edge.id.clone()) {
                    let neighbor = match direction {
                        RelationDirection::Incoming => edge.source_symbol_id.clone(),
                        RelationDirection::Outgoing => edge.target_symbol_id.clone(),
                    };
                    edges.push(edge);
                    if let Some(neighbor) = neighbor
                        && !visited.contains(&neighbor)
                    {
                        if visited.len() >= max_nodes {
                            truncated = true;
                        } else {
                            visited.insert(neighbor.clone());
                            queue.push_back((neighbor, current_depth + 1));
                        }
                    }
                }
            }
        }
        Ok((visited.into_iter().collect(), edges, truncated))
    }

    fn ensure_deadline(&self, started: Instant, deadline: Duration) -> Result<(), RetrievalError> {
        if started.elapsed() >= deadline {
            return Err(RetrievalError::DeadlineExceeded);
        }
        Ok(())
    }

    fn fresh_source(
        &self,
        path: &RelativeSourcePath,
        indexed_hash: &str,
    ) -> Result<SourceFile, RetrievalError> {
        let source = self
            .reader
            .read(path)
            .map_err(|error| source_error(path.as_str(), error))?;
        if source.content_hash() != indexed_hash {
            return Err(RetrievalError::ContentChanged(path.as_str().to_owned()));
        }
        Ok(source)
    }

    fn validate_query(&self, query: &str) -> Result<String, RetrievalError> {
        let query = query.trim();
        if query.is_empty() || query.len() > self.limits.max_query_bytes || query.contains('\0') {
            return Err(RetrievalError::InvalidQuery {
                max: self.limits.max_query_bytes,
            });
        }
        Ok(query.to_owned())
    }

    fn validate_limit(&self, limit: usize, maximum: usize) -> Result<(), RetrievalError> {
        if !(1..=maximum).contains(&limit) {
            return Err(RetrievalError::InvalidResultLimit);
        }
        Ok(())
    }

    fn validate_response_budget(&self, max_bytes: usize) -> Result<(), RetrievalError> {
        if !(512..=self.limits.max_response_bytes).contains(&max_bytes) {
            return Err(RetrievalError::InvalidResponseBudget);
        }
        Ok(())
    }

    fn base_cursor(&self, operation: &str, fingerprint: &str, sort: Vec<String>) -> CursorPayload {
        CursorPayload {
            operation: operation.to_owned(),
            repository_id: self.store.repository_id().as_str().to_owned(),
            generation_id: self.store.generation_id().as_str().to_owned(),
            query_fingerprint: fingerprint.to_owned(),
            sort,
        }
    }

    fn validate_cursor(
        &self,
        cursor: &str,
        operation: &str,
        expected_fingerprint: &str,
    ) -> Result<CursorPayload, RetrievalError> {
        let payload = decode_cursor(cursor)?;
        if payload.operation != operation {
            return Err(RetrievalError::CursorQueryMismatch);
        }
        if payload.repository_id != self.store.repository_id().as_str() {
            return Err(RetrievalError::ForeignCursor);
        }
        if payload.generation_id != self.store.generation_id().as_str() {
            return Err(RetrievalError::StaleCursor);
        }
        if payload.query_fingerprint != expected_fingerprint {
            return Err(RetrievalError::CursorQueryMismatch);
        }
        Ok(payload)
    }

    fn encode_search_cursor(&self, fingerprint: &str, key: &SearchSortKey) -> String {
        encode_cursor(&self.base_cursor(
            "search_symbols",
            fingerprint,
            vec![
                key.tier.to_string(),
                key.fts_score.to_bits().to_string(),
                key.relative_path.clone(),
                key.name.clone(),
                key.start_byte.to_string(),
                key.symbol_id.clone(),
            ],
        ))
    }

    fn decode_search_cursor(
        &self,
        cursor: &str,
        fingerprint: &str,
    ) -> Result<SearchSortKey, RetrievalError> {
        let payload = self.validate_cursor(cursor, "search_symbols", fingerprint)?;
        if payload.sort.len() != 6 {
            return Err(RetrievalError::InvalidCursor);
        }
        let tier = payload.sort[0]
            .parse::<u8>()
            .map_err(|_| RetrievalError::InvalidCursor)?;
        let score_bits = payload.sort[1]
            .parse::<u64>()
            .map_err(|_| RetrievalError::InvalidCursor)?;
        let fts_score = f64::from_bits(score_bits);
        if !fts_score.is_finite() {
            return Err(RetrievalError::InvalidCursor);
        }
        Ok(SearchSortKey {
            tier,
            fts_score,
            relative_path: payload.sort[2].clone(),
            name: payload.sort[3].clone(),
            start_byte: payload.sort[4]
                .parse::<u64>()
                .map_err(|_| RetrievalError::InvalidCursor)?,
            symbol_id: payload.sort[5].clone(),
        })
    }

    fn encode_reference_cursor(&self, fingerprint: &str, key: &ReferenceSortKey) -> String {
        encode_cursor(&self.base_cursor(
            "find_references",
            fingerprint,
            vec![
                key.relative_path.clone(),
                key.start_byte.to_string(),
                key.edge_id.clone(),
            ],
        ))
    }

    fn decode_reference_cursor(
        &self,
        cursor: &str,
        fingerprint: &str,
    ) -> Result<ReferenceSortKey, RetrievalError> {
        let payload = self.validate_cursor(cursor, "find_references", fingerprint)?;
        if payload.sort.len() != 3 {
            return Err(RetrievalError::InvalidCursor);
        }
        Ok(ReferenceSortKey {
            relative_path: payload.sort[0].clone(),
            start_byte: payload.sort[1]
                .parse::<u64>()
                .map_err(|_| RetrievalError::InvalidCursor)?,
            edge_id: payload.sort[2].clone(),
        })
    }
}

fn validate_identifier(value: &str) -> Result<(), RetrievalError> {
    if value.is_empty() || value.len() > 512 || value.contains('\0') {
        return Err(RetrievalError::InvalidQuery { max: 512 });
    }
    Ok(())
}

fn reference_sort_key(reference: &ReferenceRecord) -> ReferenceSortKey {
    ReferenceSortKey {
        relative_path: reference.relative_path.clone(),
        start_byte: reference.start_byte,
        edge_id: reference.edge_id.clone(),
    }
}

fn relation_order(left: &RelationRecord, right: &RelationRecord) -> std::cmp::Ordering {
    left.source_path
        .cmp(&right.source_path)
        .then_with(|| left.source_start_byte.cmp(&right.source_start_byte))
        .then_with(|| left.id.cmp(&right.id))
}

const fn repo_entry_rank(kind: RepoMapEntryKind) -> u8 {
    match kind {
        RepoMapEntryKind::Directory => 0,
        RepoMapEntryKind::File => 1,
    }
}

fn syntax_graph_warnings() -> Vec<String> {
    vec![
        "Relationships are syntax-derived observations, not compiler-resolved or guaranteed runtime calls."
            .to_owned(),
        "Candidate and unresolved relationships may be incomplete; inspect resolution and limitations."
            .to_owned(),
    ]
}

fn source_error(path: &str, error: RepositoryError) -> RetrievalError {
    match error {
        RepositoryError::Excluded { .. } => RetrievalError::SourceExcluded(path.to_owned()),
        RepositoryError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            RetrievalError::SourceDeleted(path.to_owned())
        }
        other => RetrievalError::SourceAccess {
            path: path.to_owned(),
            reason: other.to_string(),
        },
    }
}

fn line_excerpt(
    text: &str,
    start_line: usize,
    end_line: usize,
    max_bytes: usize,
) -> Result<(String, usize), RetrievalError> {
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    if start_line > lines.len() || end_line > lines.len() {
        return Err(RetrievalError::InvalidRange);
    }
    let mut output = String::new();
    let mut actual_end = start_line.saturating_sub(1);
    for (offset, line) in lines[start_line - 1..end_line].iter().enumerate() {
        if output.len().saturating_add(line.len()) > max_bytes {
            break;
        }
        output.push_str(line);
        actual_end = start_line + offset;
    }
    if actual_end < start_line {
        return Err(RetrievalError::ResponseTooLarge);
    }
    Ok((output, actual_end))
}

fn byte_excerpt(
    source: &SourceFile,
    start: u64,
    end: u64,
    max_bytes: usize,
) -> Result<EvidenceSnippet, RetrievalError> {
    let start = usize::try_from(start).map_err(|_| RetrievalError::InvalidRange)?;
    let mut end = usize::try_from(end).map_err(|_| RetrievalError::InvalidRange)?;
    if start >= end || end > source.text().len() || !source.text().is_char_boundary(start) {
        return Err(RetrievalError::InvalidRange);
    }
    end = end.min(start.saturating_add(max_bytes));
    while end > start && !source.text().is_char_boundary(end) {
        end -= 1;
    }
    if end == start {
        return Err(RetrievalError::ResponseTooLarge);
    }
    let start_line = source.text()[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let end_line = start_line
        + source.text()[start..end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
    Ok(EvidenceSnippet {
        relative_path: source.relative_path().as_str().to_owned(),
        content_hash: source.content_hash().to_owned(),
        start_byte: u64::try_from(start).map_err(|_| RetrievalError::InvalidRange)?,
        end_byte: u64::try_from(end).map_err(|_| RetrievalError::InvalidRange)?,
        start_line,
        end_line,
        code: source.text()[start..end].to_owned(),
    })
}

const fn ranges_overlap(left: (u64, u64), right: (u64, u64)) -> bool {
    left.0 < right.1 && right.0 < left.1
}

fn refresh_token_estimate(bundle: &mut ContextBundle) -> Result<(), RetrievalError> {
    bundle.approximate_tokens = 0;
    for _ in 0..8 {
        let bytes = serde_json::to_vec(bundle)
            .map_err(|error| RetrievalError::Serialization(error.to_string()))?
            .len();
        let estimate = bytes.div_ceil(4);
        if estimate == bundle.approximate_tokens {
            return Ok(());
        }
        bundle.approximate_tokens = estimate;
    }
    Err(RetrievalError::Serialization(
        "token estimate did not converge".to_owned(),
    ))
}

fn store_error<E: Error>(error: E) -> RetrievalError {
    RetrievalError::Store(error.to_string())
}

fn compare_search_rows(left: &SearchRow, right: &SearchRow) -> std::cmp::Ordering {
    left.tier
        .cmp(&right.tier)
        .then_with(|| left.fts_score.total_cmp(&right.fts_score))
        .then_with(|| left.symbol.relative_path.cmp(&right.symbol.relative_path))
        .then_with(|| left.symbol.name.cmp(&right.symbol.name))
        .then_with(|| left.symbol.start_byte.cmp(&right.symbol.start_byte))
        .then_with(|| left.symbol.id.cmp(&right.symbol.id))
}

fn search_hit(row: &SearchRow) -> SearchHit {
    SearchHit {
        symbol: row.symbol.clone(),
        match_kind: match row.tier {
            0 => "qualified_exact",
            1 => "exact",
            2 => "case_folded_exact",
            3 => "prefix",
            4 => "case_folded_prefix",
            5 => "identifier_tokens",
            _ => "full_text",
        }
        .to_owned(),
        fts_score: (row.tier >= 5).then_some(row.fts_score),
    }
}

fn match_kind_tier(kind: &str) -> u8 {
    match kind {
        "qualified_exact" => 0,
        "exact" => 1,
        "case_folded_exact" => 2,
        "prefix" => 3,
        "case_folded_prefix" => 4,
        "identifier_tokens" => 5,
        _ => 6,
    }
}

fn validate_filter_value(value: Option<&str>) -> Result<(), RetrievalError> {
    if value.is_some_and(|value| value.is_empty() || value.len() > 128 || value.contains('\0')) {
        return Err(RetrievalError::InvalidQuery { max: 128 });
    }
    Ok(())
}

fn validate_optional_path_prefix(value: Option<&str>) -> Result<Option<String>, RetrievalError> {
    value
        .map(|value| {
            let value = value.trim_end_matches('/');
            RelativeSourcePath::new(value.to_owned())
                .map(|path| path.as_str().to_owned())
                .map_err(|_| RetrievalError::InvalidQuery { max: 4_096 })
        })
        .transpose()
}

#[must_use]
pub fn identifier_tokens(value: &str) -> Vec<String> {
    let characters = value.chars().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut current = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        if !character.is_alphanumeric() {
            push_token(&mut output, &mut current);
            continue;
        }
        let previous = index.checked_sub(1).and_then(|prior| characters.get(prior));
        let next = characters.get(index.saturating_add(1));
        let boundary = !current.is_empty()
            && character.is_uppercase()
            && (previous.is_some_and(|value| value.is_lowercase() || value.is_numeric())
                || (previous.is_some_and(|value| value.is_uppercase())
                    && next.is_some_and(|value| value.is_lowercase())));
        if boundary {
            push_token(&mut output, &mut current);
        }
        current.push(character);
    }
    push_token(&mut output, &mut current);
    output
}

fn push_token(output: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        output.push(current.to_lowercase());
        current.clear();
    }
}

fn literal_fts_expression(tokens: &[String]) -> String {
    if tokens.is_empty() {
        return "\"__codeatlas_no_match__\"".to_owned();
    }
    tokens
        .iter()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn escape_like_prefix(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len().saturating_add(1));
    for character in value.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped.push('%');
    escaped
}

fn fingerprint(parts: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

fn encode_cursor(payload: &CursorPayload) -> String {
    let mut parts = vec![
        CURSOR_VERSION.to_owned(),
        hex_encode(payload.operation.as_bytes()),
        hex_encode(payload.repository_id.as_bytes()),
        hex_encode(payload.generation_id.as_bytes()),
        payload.query_fingerprint.clone(),
    ];
    parts.extend(
        payload
            .sort
            .iter()
            .map(|value| hex_encode(value.as_bytes())),
    );
    parts.join(".")
}

fn decode_cursor(cursor: &str) -> Result<CursorPayload, RetrievalError> {
    if cursor.is_empty() || cursor.len() > MAX_CURSOR_BYTES || cursor.contains('\0') {
        return Err(RetrievalError::InvalidCursor);
    }
    let parts = cursor.split('.').collect::<Vec<_>>();
    if parts.len() < 5
        || parts[0] != CURSOR_VERSION
        || parts[4].len() != 64
        || !parts[4]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(RetrievalError::InvalidCursor);
    }
    Ok(CursorPayload {
        operation: hex_decode_utf8(parts[1])?,
        repository_id: hex_decode_utf8(parts[2])?,
        generation_id: hex_decode_utf8(parts[3])?,
        query_fingerprint: parts[4].to_owned(),
        sort: parts[5..]
            .iter()
            .map(|value| hex_decode_utf8(value))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

/// Exercises the bounded opaque-cursor decoder without selecting a repository.
///
/// This is intentionally a syntax-only entry point for fuzzing. Normal callers must
/// still use `RetrievalService`, which additionally binds cursors to an operation,
/// repository, generation and query fingerprint.
#[doc(hidden)]
pub fn validate_opaque_cursor_syntax(cursor: &str) -> Result<(), RetrievalError> {
    decode_cursor(cursor).map(drop)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn hex_decode_utf8(value: &str) -> Result<String, RetrievalError> {
    if !value.len().is_multiple_of(2) {
        return Err(RetrievalError::InvalidCursor);
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().as_chunks::<2>().0 {
        let high = hex_digit(pair[0]).ok_or(RetrievalError::InvalidCursor)?;
        let low = hex_digit(pair[1]).ok_or(RetrievalError::InvalidCursor)?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes).map_err(|_| RetrievalError::InvalidCursor)
}

const fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

#[derive(Serialize)]
struct TextBlock<'a> {
    r#type: &'static str,
    text: &'a str,
}

#[derive(Serialize)]
struct WireProjection<'a, T> {
    #[serde(rename = "structuredContent")]
    structured_content: &'a T,
    content: [TextBlock<'a>; 1],
}

fn fit_budget<T, F>(
    value: &mut T,
    max_bytes: usize,
    mut reduce: F,
) -> Result<Budgeted<T>, RetrievalError>
where
    T: Serialize + Clone,
    F: FnMut(&mut T) -> bool,
{
    loop {
        let structured_json = serde_json::to_string(value)
            .map_err(|error| RetrievalError::Serialization(error.to_string()))?;
        let projection = WireProjection {
            structured_content: value,
            content: [TextBlock {
                r#type: "text",
                text: &structured_json,
            }],
        };
        let serialized_bytes = serde_json::to_vec(&projection)
            .map_err(|error| RetrievalError::Serialization(error.to_string()))?
            .len();
        if serialized_bytes <= max_bytes {
            return Ok(Budgeted {
                data: value.clone(),
                structured_json: structured_json.clone(),
                text_fallback: structured_json,
                serialized_bytes,
            });
        }
        if !reduce(value) {
            return Err(RetrievalError::ResponseTooLarge);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        convert::Infallible,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use ca_core::RepositoryRoot;

    use super::*;
    use crate::repository::{RepositoryIdentity, ScanPolicy};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    #[derive(Default)]
    struct FakeStore {
        repository_id: Option<RepositoryId>,
        generation_id: Option<GenerationId>,
        rows: Vec<SearchRow>,
        symbols: BTreeMap<String, SymbolRecord>,
        references: Vec<ReferenceRecord>,
        relations: Vec<RelationRecord>,
        files: Vec<IndexedFileRecord>,
    }

    impl FakeStore {
        fn new(repository: &str, generation: &str) -> Self {
            Self {
                repository_id: Some(RepositoryId::new(repository).expect("valid repository ID")),
                generation_id: Some(GenerationId::new(generation).expect("valid generation ID")),
                ..Self::default()
            }
        }
    }

    impl RetrievalStore for FakeStore {
        type Error = Infallible;

        fn repository_id(&self) -> &RepositoryId {
            self.repository_id.as_ref().expect("repository ID")
        }

        fn generation_id(&self) -> &GenerationId {
            self.generation_id.as_ref().expect("generation ID")
        }

        fn coverage(&self) -> Result<CoverageSummary, Self::Error> {
            Ok(CoverageSummary {
                status: "syntax_and_name_resolution".to_owned(),
                warning_count: 1,
                unresolved_occurrences: 2,
                resolver_version: "resolver-test-v1".to_owned(),
            })
        }

        fn search_symbols(&self, plan: &SearchPlan) -> Result<Vec<SearchRow>, Self::Error> {
            let mut rows = self.rows.clone();
            rows.sort_by(compare_search_rows);
            if let Some(after) = &plan.after {
                rows.retain(|row| {
                    compare_search_rows(
                        row,
                        &SearchRow {
                            symbol: SymbolRecord {
                                id: after.symbol_id.clone(),
                                relative_path: after.relative_path.clone(),
                                language: String::new(),
                                kind: String::new(),
                                name: after.name.clone(),
                                container: None,
                                signature: None,
                                start_byte: after.start_byte,
                                end_byte: after.start_byte,
                                syntax_start_byte: after.start_byte,
                                syntax_end_byte: after.start_byte,
                                content_hash: String::new(),
                                parse_status: String::new(),
                                attributes: Vec::new(),
                                limitations: Vec::new(),
                            },
                            tier: after.tier,
                            fts_score: after.fts_score,
                        },
                    )
                    .is_gt()
                });
            }
            rows.truncate(plan.fetch_limit);
            Ok(rows)
        }

        fn symbol(&self, symbol_id: &str) -> Result<Option<SymbolRecord>, Self::Error> {
            Ok(self.symbols.get(symbol_id).cloned())
        }

        fn file_symbols(
            &self,
            relative_path: &str,
            limit: usize,
        ) -> Result<Vec<SymbolRecord>, Self::Error> {
            Ok(self
                .symbols
                .values()
                .filter(|symbol| symbol.relative_path == relative_path)
                .take(limit)
                .cloned()
                .collect())
        }

        fn references_to(
            &self,
            _symbol_id: &str,
            include_candidates: bool,
            after: Option<&ReferenceSortKey>,
            limit: usize,
        ) -> Result<Vec<ReferenceRecord>, Self::Error> {
            let mut references = self
                .references
                .iter()
                .filter(|reference| include_candidates || reference.resolution != "candidate")
                .filter(|reference| {
                    after.is_none_or(|after| reference_sort_key(reference) > *after)
                })
                .cloned()
                .collect::<Vec<_>>();
            references.sort_by_key(reference_sort_key);
            references.truncate(limit);
            Ok(references)
        }

        fn adjacent_relations(
            &self,
            symbol_id: &str,
            direction: RelationDirection,
            filter: RelationFilter,
            include_candidates: bool,
            limit: usize,
        ) -> Result<Vec<RelationRecord>, Self::Error> {
            let mut relations = self
                .relations
                .iter()
                .filter(|relation| include_candidates || relation.resolution != "candidate")
                .filter(|relation| match filter {
                    RelationFilter::Any => true,
                    RelationFilter::Calls => relation.relationship == "calls",
                    RelationFilter::References => relation.relationship == "references",
                })
                .filter(|relation| match direction {
                    RelationDirection::Incoming => {
                        relation.target_symbol_id.as_deref() == Some(symbol_id)
                    }
                    RelationDirection::Outgoing => {
                        relation.source_symbol_id.as_deref() == Some(symbol_id)
                    }
                })
                .take(limit)
                .cloned()
                .collect::<Vec<_>>();
            relations.sort_by(relation_order);
            Ok(relations)
        }

        fn incoming_relations_to_path(
            &self,
            relative_path: &str,
            include_candidates: bool,
            limit: usize,
        ) -> Result<Vec<RelationRecord>, Self::Error> {
            Ok(self
                .relations
                .iter()
                .filter(|relation| include_candidates || relation.resolution != "candidate")
                .filter(|relation| relation.target_path.as_deref() == Some(relative_path))
                .take(limit)
                .cloned()
                .collect())
        }

        fn repository_files(
            &self,
            path_prefix: Option<&str>,
            limit: usize,
        ) -> Result<Vec<IndexedFileRecord>, Self::Error> {
            Ok(self
                .files
                .iter()
                .filter(|file| {
                    path_prefix.is_none_or(|prefix| {
                        file.relative_path == prefix
                            || file.relative_path.starts_with(&format!("{prefix}/"))
                    })
                })
                .take(limit)
                .cloned()
                .collect())
        }

        fn indexed_file(
            &self,
            relative_path: &str,
        ) -> Result<Option<IndexedFileRecord>, Self::Error> {
            Ok(self
                .files
                .iter()
                .find(|file| file.relative_path == relative_path)
                .cloned())
        }
    }

    fn test_reader(label: &str) -> (PathBuf, SourceReader) {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "codeatlas-retrieval-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create test repository");
        let root_value = RepositoryRoot::new(&root).expect("authorize test repository");
        let identity = RepositoryIdentity::derive(root_value).expect("derive repository identity");
        (root, SourceReader::new(identity, ScanPolicy::default()))
    }

    fn symbol(id: &str, name: &str, path: &str, start: u64, end: u64) -> SymbolRecord {
        SymbolRecord {
            id: id.to_owned(),
            relative_path: path.to_owned(),
            language: "rust".to_owned(),
            kind: "function".to_owned(),
            name: name.to_owned(),
            container: None,
            signature: Some(format!("fn {name}()")),
            start_byte: start,
            end_byte: end,
            syntax_start_byte: start,
            syntax_end_byte: end,
            content_hash: "indexed".to_owned(),
            parse_status: "complete".to_owned(),
            attributes: Vec::new(),
            limitations: Vec::new(),
        }
    }

    fn relation(id: &str, source: &str, target: &str) -> RelationRecord {
        RelationRecord {
            id: id.to_owned(),
            relationship: "calls".to_owned(),
            source_symbol_id: Some(source.to_owned()),
            source_observation_id: format!("call-{id}"),
            source_path: format!("src/{source}.rs"),
            source_start_byte: 1,
            target_symbol_id: Some(target.to_owned()),
            target_path: Some(format!("src/{target}.rs")),
            resolution: "lexically_resolved".to_owned(),
            candidate_count: 1,
            rule_version: "rule-v1".to_owned(),
            reason: "test".to_owned(),
            limitations: Vec::new(),
        }
    }

    #[test]
    fn identifier_queries_are_split_without_fts_syntax() {
        assert_eq!(
            identifier_tokens("HTTPServer_valueÉclair"),
            ["http", "server", "value", "éclair"]
        );
        assert!(identifier_tokens("::()%").is_empty());
        assert_eq!(
            literal_fts_expression(&identifier_tokens("getHTTP_server")),
            "\"get\" AND \"http\" AND \"server\""
        );
    }

    #[test]
    fn serialized_budget_counts_json_escaping_and_text_duplication() {
        let mut excerpt = CodeExcerpt {
            repository_id: "repo-budget".to_owned(),
            generation_id: "generation-budget".to_owned(),
            relative_path: "src/escaped.rs".to_owned(),
            content_hash: "a".repeat(64),
            start_line: 1,
            end_line: 1,
            code: "\"\\\n".repeat(1_000),
            truncated: false,
        };
        let budgeted = fit_budget(&mut excerpt, 700, |value| {
            if value.code.pop().is_none() {
                return false;
            }
            value.truncated = true;
            true
        })
        .expect("escaped response fits after structural reduction");
        assert!(budgeted.serialized_bytes <= 700);
        assert!(budgeted.data.truncated);
        let projection = serde_json::json!({
            "structuredContent": budgeted.data,
            "content": [{"type": "text", "text": budgeted.text_fallback}],
        });
        assert_eq!(
            serde_json::to_vec(&projection)
                .expect("serialize verification projection")
                .len(),
            budgeted.serialized_bytes
        );
    }

    #[test]
    fn generated_cursor_and_response_budget_properties_hold() {
        for seed in 0_u64..512 {
            let payload = CursorPayload {
                operation: format!("operation-{seed}"),
                repository_id: format!("repository-{seed}"),
                generation_id: format!("generation-{}", seed.wrapping_mul(17)),
                query_fingerprint: format!("{seed:064x}"),
                sort: vec![
                    format!("src/{seed}.rs"),
                    seed.wrapping_mul(seed).to_string(),
                    format!("symbol-{seed}"),
                ],
            };
            let encoded = encode_cursor(&payload);
            assert!(encoded.len() <= MAX_CURSOR_BYTES);
            assert_eq!(
                decode_cursor(&encoded).expect("decode generated cursor"),
                payload
            );
            assert!(validate_opaque_cursor_syntax(&encoded).is_ok());
        }
        let invalid_fingerprint = format!("ca1.61.62.63.{}", "g".repeat(64));
        assert!(validate_opaque_cursor_syntax(&invalid_fingerprint).is_err());
        assert!(validate_opaque_cursor_syntax(&"a".repeat(MAX_CURSOR_BYTES + 1)).is_err());

        for (index, pattern) in ["plain", "\"quoted\"", "\\slash", "éclair", "\nline"]
            .into_iter()
            .enumerate()
        {
            for budget in [512, 768, 1_024, 2_048, 4_096] {
                let mut excerpt = CodeExcerpt {
                    repository_id: format!("repo-{index}"),
                    generation_id: format!("generation-{index}"),
                    relative_path: format!("src/{index}.rs"),
                    content_hash: "a".repeat(64),
                    start_line: 1,
                    end_line: 1,
                    code: pattern.repeat(1_000),
                    truncated: false,
                };
                match fit_budget(&mut excerpt, budget, |value| {
                    if value.code.is_empty() {
                        return false;
                    }
                    let mut next = value.code.len() / 2;
                    while next > 0 && !value.code.is_char_boundary(next) {
                        next -= 1;
                    }
                    value.code.truncate(next);
                    value.truncated = true;
                    true
                }) {
                    Ok(result) => {
                        assert!(result.serialized_bytes <= budget);
                        assert!(
                            serde_json::from_str::<serde_json::Value>(&result.structured_json)
                                .is_ok()
                        );
                    }
                    Err(RetrievalError::ResponseTooLarge) => {
                        let mut empty = excerpt.clone();
                        empty.code.clear();
                        let structured_json =
                            serde_json::to_string(&empty).expect("serialize empty projection");
                        let projection = WireProjection {
                            structured_content: &empty,
                            content: [TextBlock {
                                r#type: "text",
                                text: &structured_json,
                            }],
                        };
                        assert!(
                            serde_json::to_vec(&projection)
                                .expect("serialize empty wire projection")
                                .len()
                                > budget
                        );
                    }
                    Err(other) => panic!("unexpected budget property error: {other}"),
                }
            }
        }
    }

    #[test]
    fn search_ranking_cursors_and_wire_bytes_are_deterministic() {
        let (root, reader) = test_reader("search");
        let mut store = FakeStore::new("repo-a", "generation-a");
        store.rows = vec![
            SearchRow {
                symbol: symbol("full", "Alpha helper", "src/z.rs", 10, 20),
                tier: 6,
                fts_score: -9.0,
            },
            SearchRow {
                symbol: symbol("exact-a", "Alpha", "src/a.rs", 1, 5),
                tier: 1,
                fts_score: 0.0,
            },
            SearchRow {
                symbol: symbol("exact-b", "Alpha", "src/b.rs", 1, 5),
                tier: 1,
                fts_score: 0.0,
            },
        ];
        let service = RetrievalService::new(&store, &reader, RetrievalLimits::default())
            .expect("create retrieval service");
        let first = service
            .search_symbols(SearchRequest {
                query: "Alpha".to_owned(),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: None,
                max_bytes: 4_096,
            })
            .expect("first search page");
        assert_eq!(first.data.results[0].symbol.id, "exact-a");
        assert!(first.serialized_bytes <= 4_096);
        serde_json::from_str::<serde_json::Value>(&first.structured_json)
            .expect("structured response is valid JSON");
        let cursor = first.data.next_cursor.expect("continuation cursor");
        let second = service
            .search_symbols(SearchRequest {
                query: "Alpha".to_owned(),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: Some(cursor.clone()),
                max_bytes: 4_096,
            })
            .expect("second search page");
        assert_eq!(second.data.results[0].symbol.id, "exact-b");
        assert!(matches!(
            service.search_symbols(SearchRequest {
                query: "Beta".to_owned(),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: Some(cursor.clone()),
                max_bytes: 4_096,
            }),
            Err(RetrievalError::CursorQueryMismatch)
        ));
        assert!(matches!(
            service.search_symbols(SearchRequest {
                query: "Alpha".to_owned(),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: Some("not-a-cursor".to_owned()),
                max_bytes: 4_096,
            }),
            Err(RetrievalError::InvalidCursor)
        ));
        assert!(matches!(
            service.search_symbols(SearchRequest {
                query: "   ".to_owned(),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: None,
                max_bytes: 4_096,
            }),
            Err(RetrievalError::InvalidQuery { .. })
        ));
        assert!(matches!(
            service.search_symbols(SearchRequest {
                query: "x".repeat(MAX_QUERY_BYTES + 1),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: None,
                max_bytes: 4_096,
            }),
            Err(RetrievalError::InvalidQuery { .. })
        ));
        let stale = FakeStore::new("repo-a", "generation-b");
        let stale_service = RetrievalService::new(&stale, &reader, RetrievalLimits::default())
            .expect("create stale service");
        assert!(matches!(
            stale_service.search_symbols(SearchRequest {
                query: "Alpha".to_owned(),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: Some(cursor.clone()),
                max_bytes: 4_096,
            }),
            Err(RetrievalError::StaleCursor)
        ));
        let foreign = FakeStore::new("repo-b", "generation-a");
        let foreign_service = RetrievalService::new(&foreign, &reader, RetrievalLimits::default())
            .expect("create foreign service");
        assert!(matches!(
            foreign_service.search_symbols(SearchRequest {
                query: "Alpha".to_owned(),
                filter: SearchFilter {
                    language: None,
                    kind: None,
                    path_prefix: None,
                },
                limit: 1,
                cursor: Some(cursor),
                max_bytes: 4_096,
            }),
            Err(RetrievalError::ForeignCursor)
        ));
        fs::remove_dir_all(root).expect("remove test repository");
    }

    #[test]
    fn graph_cycles_and_fanout_stop_at_explicit_limits() {
        let (root, reader) = test_reader("graph");
        let mut store = FakeStore::new("repo-graph", "generation-graph");
        for id in ["a", "b", "c"] {
            store
                .symbols
                .insert(id.to_owned(), symbol(id, id, &format!("src/{id}.rs"), 0, 1));
        }
        store.relations = vec![
            relation("a-b", "a", "b"),
            relation("b-c", "b", "c"),
            relation("c-a", "c", "a"),
        ];
        let mut candidate = relation("a-c-candidate", "a", "c");
        candidate.resolution = "candidate".to_owned();
        candidate.candidate_count = 2;
        candidate.limitations = vec!["ambiguous_candidate_set".to_owned()];
        store.relations.push(candidate);
        let service = RetrievalService::new(&store, &reader, RetrievalLimits::default())
            .expect("create retrieval service");
        let trace = service
            .trace_calls(TraceRequest {
                symbol_id: "a".to_owned(),
                direction: RelationDirection::Outgoing,
                depth: 8,
                include_candidates: false,
                max_nodes: 3,
                max_edges: 2,
                deadline: Duration::from_secs(1),
                max_bytes: 8_192,
            })
            .expect("bounded cyclic trace");
        assert_eq!(trace.data.edges.len(), 2);
        assert!(trace.data.truncated);
        assert!(
            trace
                .data
                .warnings
                .iter()
                .any(|warning| warning.contains("syntax-derived"))
        );
        assert!(
            trace
                .data
                .edges
                .iter()
                .all(|edge| edge.resolution != "candidate")
        );
        let with_candidates = service
            .trace_calls(TraceRequest {
                symbol_id: "a".to_owned(),
                direction: RelationDirection::Outgoing,
                depth: 1,
                include_candidates: true,
                max_nodes: 3,
                max_edges: 4,
                deadline: Duration::from_secs(1),
                max_bytes: 8_192,
            })
            .expect("trace with candidates");
        let disclosed = with_candidates
            .data
            .edges
            .iter()
            .find(|edge| edge.resolution == "candidate")
            .expect("candidate edge is explicitly disclosed");
        assert_eq!(disclosed.candidate_count, 2);
        assert_eq!(disclosed.limitations, ["ambiguous_candidate_set"]);
        fs::remove_dir_all(root).expect("remove test repository");
    }

    #[test]
    fn fresh_reads_and_context_dedup_overlapping_snippets() {
        let (root, reader) = test_reader("context");
        let text = "fn alpha() {\n    beta();\n}\n";
        let source_path = root.join("src/lib.rs");
        fs::create_dir_all(source_path.parent().expect("source parent"))
            .expect("create source parent");
        fs::write(&source_path, text).expect("write source");
        let hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        let mut store = FakeStore::new("repo-context", "generation-context");
        store.files.push(IndexedFileRecord {
            relative_path: "src/lib.rs".to_owned(),
            content_hash: hash.clone(),
            language: "rust".to_owned(),
            parse_status: "complete".to_owned(),
            coverage: "{}".to_owned(),
            byte_length: text.len() as u64,
            symbol_count: 2,
        });
        store.files.push(IndexedFileRecord {
            relative_path: "target/secret.rs".to_owned(),
            content_hash: hash.clone(),
            language: "rust".to_owned(),
            parse_status: "complete".to_owned(),
            coverage: "{}".to_owned(),
            byte_length: 1,
            symbol_count: 0,
        });
        let mut alpha = symbol("alpha", "alpha", "src/lib.rs", 0, text.len() as u64);
        alpha.content_hash.clone_from(&hash);
        let mut beta = symbol("beta", "beta", "src/lib.rs", 10, 24);
        beta.content_hash.clone_from(&hash);
        store.symbols.insert(alpha.id.clone(), alpha.clone());
        store.symbols.insert(beta.id.clone(), beta.clone());
        store.rows = vec![
            SearchRow {
                symbol: alpha,
                tier: 1,
                fts_score: 0.0,
            },
            SearchRow {
                symbol: beta,
                tier: 5,
                fts_score: -1.0,
            },
        ];
        let service = RetrievalService::new(&store, &reader, RetrievalLimits::default())
            .expect("create retrieval service");
        let context = service
            .build_context(ContextRequest {
                query: "alpha".to_owned(),
                scope: Some("src".to_owned()),
                include_candidates: true,
                max_bytes: 4_096,
            })
            .expect("build bounded context");
        assert_eq!(context.data.snippets.len(), 1);
        assert_eq!(
            context.data.token_estimate_method,
            "ceil(UTF-8 JSON bytes / 4)"
        );
        assert_eq!(
            context.data.approximate_tokens,
            serde_json::to_vec(&context.data)
                .expect("serialize context for token estimate")
                .len()
                .div_ceil(4)
        );
        assert!(context.serialized_bytes <= 4_096);

        let read = service
            .read_code(ReadCodeRequest {
                relative_path: "src/lib.rs".to_owned(),
                start_line: 1,
                end_line: 2,
                expected_hash: hash.clone(),
                max_bytes: 2_048,
            })
            .expect("read unchanged indexed source");
        assert!(read.data.code.contains("beta"));
        fs::write(&source_path, "fn changed() {}\n").expect("change source");
        assert!(matches!(
            service.read_code(ReadCodeRequest {
                relative_path: "src/lib.rs".to_owned(),
                start_line: 1,
                end_line: 1,
                expected_hash: hash.clone(),
                max_bytes: 2_048,
            }),
            Err(RetrievalError::ContentChanged(_))
        ));
        fs::remove_file(&source_path).expect("delete source");
        assert!(matches!(
            service.read_code(ReadCodeRequest {
                relative_path: "src/lib.rs".to_owned(),
                start_line: 1,
                end_line: 1,
                expected_hash: hash,
                max_bytes: 2_048,
            }),
            Err(RetrievalError::SourceDeleted(_))
        ));
        assert!(matches!(
            service.read_code(ReadCodeRequest {
                relative_path: "target/secret.rs".to_owned(),
                start_line: 1,
                end_line: 1,
                expected_hash: blake3::hash(text.as_bytes()).to_hex().to_string(),
                max_bytes: 2_048,
            }),
            Err(RetrievalError::SourceExcluded(_))
        ));
        fs::remove_dir_all(root).expect("remove test repository");
    }
}
