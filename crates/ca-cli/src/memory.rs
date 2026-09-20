use ca_engine::memory::{
    EvidenceStatus, MemoryDraft, MemoryEvidence, MemoryKind, MemoryRecord, MemorySearch,
    MemoryStore,
};
use ca_storage::{
    MemoryDraft as StoredMemoryDraft, MemoryEvidence as StoredMemoryEvidence,
    MemoryEvidenceStatus as StoredEvidenceStatus, MemoryKind as StoredMemoryKind,
    MemoryRecord as StoredMemoryRecord, MemorySearch as StoredMemorySearch, Storage, StorageError,
};

pub(crate) struct StorageMemoryAdapter<'a> {
    storage: &'a Storage,
}

impl<'a> StorageMemoryAdapter<'a> {
    pub(crate) const fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }
}

impl MemoryStore for StorageMemoryAdapter<'_> {
    type Error = StorageError;

    fn search(&self, request: &MemorySearch) -> Result<Vec<MemoryRecord>, Self::Error> {
        self.storage
            .search_memories(&StoredMemorySearch {
                query: request.query.clone(),
                scope: request.scope.clone(),
                include_stale: request.include_stale,
                limit: request.limit,
            })
            .map(|records| records.into_iter().map(map_record).collect())
    }

    fn upsert(&self, draft: MemoryDraft) -> Result<MemoryRecord, Self::Error> {
        self.storage
            .upsert_memory(StoredMemoryDraft {
                memory_id: draft.memory_id,
                text: draft.text,
                kind: map_kind(draft.kind),
                author: draft.author,
                origin: draft.origin,
                scope: draft.scope,
                evidence: draft
                    .evidence
                    .into_iter()
                    .map(|item| StoredMemoryEvidence {
                        relative_path: item.relative_path,
                        content_hash: item.content_hash,
                        symbol_id: item.symbol_id,
                    })
                    .collect(),
                expected_revision: draft.expected_revision,
            })
            .map(map_record)
    }

    fn forget(&self, id: String, expected_revision: u64) -> Result<MemoryRecord, Self::Error> {
        self.storage
            .forget_memory(id, expected_revision)
            .map(map_record)
    }

    fn get(&self, id: &str) -> Result<Option<MemoryRecord>, Self::Error> {
        self.storage.memory(id).map(|record| record.map(map_record))
    }
}

fn map_kind(kind: MemoryKind) -> StoredMemoryKind {
    match kind {
        MemoryKind::Decision => StoredMemoryKind::Decision,
        MemoryKind::Convention => StoredMemoryKind::Convention,
        MemoryKind::Pitfall => StoredMemoryKind::Pitfall,
        MemoryKind::Task => StoredMemoryKind::Task,
    }
}

fn map_record(record: StoredMemoryRecord) -> MemoryRecord {
    MemoryRecord {
        id: record.id,
        text: record.text,
        kind: match record.kind {
            StoredMemoryKind::Decision => MemoryKind::Decision,
            StoredMemoryKind::Convention => MemoryKind::Convention,
            StoredMemoryKind::Pitfall => MemoryKind::Pitfall,
            StoredMemoryKind::Task => MemoryKind::Task,
        },
        revision: record.revision,
        author: record.author,
        origin: record.origin,
        scope: record.scope,
        created_at: record.created_at,
        updated_at: record.updated_at,
        evidence: record
            .evidence
            .into_iter()
            .map(|item| MemoryEvidence {
                relative_path: item.relative_path,
                content_hash: item.content_hash,
                symbol_id: item.symbol_id,
            })
            .collect(),
        evidence_status: match record.evidence_status {
            StoredEvidenceStatus::Verified => EvidenceStatus::Verified,
            StoredEvidenceStatus::Unverified => EvidenceStatus::Unverified,
            StoredEvidenceStatus::Stale => EvidenceStatus::Stale,
        },
    }
}
