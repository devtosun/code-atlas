use std::{error::Error, fmt};

use serde::Serialize;

pub const MAX_MEMORY_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_MEMORY_METADATA_BYTES: usize = 256;
pub const MAX_MEMORY_EVIDENCE: usize = 32;
pub const MAX_MEMORY_EVIDENCE_BYTES: usize = 8 * 1024;
pub const MAX_MEMORY_SEARCH_BYTES: usize = 256;
pub const MAX_MEMORY_RESULTS: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Verified,
    Unverified,
    Stale,
}

impl EvidenceStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Stale => "stale",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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
    pub evidence_status: EvidenceStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySearch {
    pub query: String,
    pub scope: Option<String>,
    pub include_stale: bool,
    pub limit: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryLimits {
    pub max_text_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_evidence: usize,
    pub max_evidence_bytes: usize,
    pub max_search_bytes: usize,
    pub max_results: usize,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: MAX_MEMORY_TEXT_BYTES,
            max_metadata_bytes: MAX_MEMORY_METADATA_BYTES,
            max_evidence: MAX_MEMORY_EVIDENCE,
            max_evidence_bytes: MAX_MEMORY_EVIDENCE_BYTES,
            max_search_bytes: MAX_MEMORY_SEARCH_BYTES,
            max_results: MAX_MEMORY_RESULTS,
        }
    }
}

pub trait MemoryStore {
    type Error: Error + Send + Sync + 'static;

    fn search(&self, request: &MemorySearch) -> Result<Vec<MemoryRecord>, Self::Error>;
    fn upsert(&self, draft: MemoryDraft) -> Result<MemoryRecord, Self::Error>;
    fn forget(&self, id: String, expected_revision: u64) -> Result<MemoryRecord, Self::Error>;
    fn get(&self, id: &str) -> Result<Option<MemoryRecord>, Self::Error>;
}

#[derive(Debug)]
pub enum MemoryError<E> {
    InvalidLimits,
    InvalidInput(String),
    Store(E),
}

impl<E: fmt::Display> fmt::Display for MemoryError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid memory limits"),
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::Store(error) => write!(formatter, "memory store operation failed: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for MemoryError<E> {}

pub struct MemoryService<'a, S> {
    store: &'a S,
    limits: MemoryLimits,
}

impl<'a, S: MemoryStore> MemoryService<'a, S> {
    pub fn new(store: &'a S, limits: MemoryLimits) -> Result<Self, MemoryError<S::Error>> {
        if limits.max_text_bytes == 0
            || limits.max_text_bytes > MAX_MEMORY_TEXT_BYTES
            || limits.max_metadata_bytes == 0
            || limits.max_metadata_bytes > MAX_MEMORY_METADATA_BYTES
            || limits.max_evidence == 0
            || limits.max_evidence > MAX_MEMORY_EVIDENCE
            || limits.max_evidence_bytes == 0
            || limits.max_evidence_bytes > MAX_MEMORY_EVIDENCE_BYTES
            || limits.max_search_bytes == 0
            || limits.max_search_bytes > MAX_MEMORY_SEARCH_BYTES
            || limits.max_results == 0
            || limits.max_results > MAX_MEMORY_RESULTS
        {
            return Err(MemoryError::InvalidLimits);
        }
        Ok(Self { store, limits })
    }

    pub fn search(
        &self,
        request: MemorySearch,
    ) -> Result<Vec<MemoryRecord>, MemoryError<S::Error>> {
        validate_text(
            "memory search query",
            &request.query,
            self.limits.max_search_bytes,
        )?;
        if request.limit == 0 || request.limit > self.limits.max_results {
            return Err(MemoryError::InvalidInput(format!(
                "memory search limit must be in 1..={}",
                self.limits.max_results
            )));
        }
        if let Some(scope) = request.scope.as_deref() {
            validate_text("memory search scope", scope, self.limits.max_metadata_bytes)?;
        }
        self.store.search(&request).map_err(MemoryError::Store)
    }

    pub fn upsert(&self, draft: MemoryDraft) -> Result<MemoryRecord, MemoryError<S::Error>> {
        validate_body(&draft.text, self.limits.max_text_bytes)?;
        validate_text(
            "memory author",
            &draft.author,
            self.limits.max_metadata_bytes,
        )?;
        validate_text(
            "memory origin",
            &draft.origin,
            self.limits.max_metadata_bytes,
        )?;
        validate_text("memory scope", &draft.scope, self.limits.max_metadata_bytes)?;
        if draft.evidence.len() > self.limits.max_evidence {
            return Err(MemoryError::InvalidInput(format!(
                "memory evidence exceeds the {}-item limit",
                self.limits.max_evidence
            )));
        }
        let evidence_bytes = draft.evidence.iter().try_fold(0_usize, |total, item| {
            total
                .checked_add(item.relative_path.len())?
                .checked_add(item.content_hash.len())?
                .checked_add(item.symbol_id.as_deref().map_or(0, str::len))
        });
        if evidence_bytes.is_none_or(|bytes| bytes > self.limits.max_evidence_bytes) {
            return Err(MemoryError::InvalidInput(format!(
                "memory evidence exceeds the {}-byte aggregate limit",
                self.limits.max_evidence_bytes
            )));
        }
        if draft.expected_revision == Some(0) {
            return Err(MemoryError::InvalidInput(
                "expected_revision must be at least one".to_owned(),
            ));
        }
        self.store.upsert(draft).map_err(MemoryError::Store)
    }

    pub fn forget(
        &self,
        id: String,
        expected_revision: u64,
    ) -> Result<MemoryRecord, MemoryError<S::Error>> {
        validate_id(&id)?;
        if expected_revision == 0 {
            return Err(MemoryError::InvalidInput(
                "expected_revision must be at least one".to_owned(),
            ));
        }
        self.store
            .forget(id, expected_revision)
            .map_err(MemoryError::Store)
    }

    pub fn get(&self, id: &str) -> Result<Option<MemoryRecord>, MemoryError<S::Error>> {
        validate_id(id)?;
        self.store.get(id).map_err(MemoryError::Store)
    }
}

fn validate_text<E>(label: &str, value: &str, maximum: usize) -> Result<(), MemoryError<E>> {
    if value.is_empty()
        || value.len() > maximum
        || value.contains('\0')
        || value.chars().any(char::is_control)
    {
        return Err(MemoryError::InvalidInput(format!(
            "{label} must contain 1..={maximum} UTF-8 bytes and no control characters"
        )));
    }
    Ok(())
}

fn validate_body<E>(value: &str, maximum: usize) -> Result<(), MemoryError<E>> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(MemoryError::InvalidInput(format!(
            "memory text must contain 1..={maximum} UTF-8 bytes and no NUL"
        )));
    }
    Ok(())
}

fn validate_id<E>(id: &str) -> Result<(), MemoryError<E>> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(MemoryError::InvalidInput(
            "memory ID is invalid or exceeds policy".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeError;

    impl fmt::Display for FakeError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("fake store error")
        }
    }

    impl Error for FakeError {}

    struct FakeStore;

    impl MemoryStore for FakeStore {
        type Error = FakeError;

        fn search(&self, _request: &MemorySearch) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        fn upsert(&self, _draft: MemoryDraft) -> Result<MemoryRecord, Self::Error> {
            Err(FakeError)
        }

        fn forget(
            &self,
            _id: String,
            _expected_revision: u64,
        ) -> Result<MemoryRecord, Self::Error> {
            Err(FakeError)
        }

        fn get(&self, _id: &str) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    #[test]
    fn limits_reject_oversized_notes_before_the_store() {
        let store = FakeStore;
        let service = MemoryService::new(&store, MemoryLimits::default()).expect("valid limits");
        let result = service.upsert(MemoryDraft {
            memory_id: None,
            text: "x".repeat(MAX_MEMORY_TEXT_BYTES + 1),
            kind: MemoryKind::Task,
            author: "test".to_owned(),
            origin: "test".to_owned(),
            scope: "repository".to_owned(),
            evidence: Vec::new(),
            expected_revision: None,
        });
        assert!(matches!(result, Err(MemoryError::InvalidInput(_))));
    }

    #[test]
    fn search_and_get_validate_bounded_inputs() {
        let store = FakeStore;
        let service = MemoryService::new(&store, MemoryLimits::default()).expect("valid limits");
        assert!(
            service
                .search(MemorySearch {
                    query: "decision".to_owned(),
                    scope: None,
                    include_stale: false,
                    limit: 20,
                })
                .expect("valid search")
                .is_empty()
        );
        assert!(matches!(
            service.get("../escape"),
            Err(MemoryError::InvalidInput(_))
        ));
    }
}
