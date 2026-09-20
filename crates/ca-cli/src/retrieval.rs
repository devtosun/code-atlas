use ca_engine::retrieval::{
    CoverageSummary, IndexedFileRecord, ReferenceRecord, ReferenceSortKey, RelationDirection,
    RelationFilter, RelationRecord, RetrievalStore, SearchPlan, SearchRow, SearchSortKey,
    SymbolRecord,
};
use ca_storage::{
    ReadSnapshot, StorageError, StoredGraphDirection, StoredIndexedFile, StoredReferenceAfter,
    StoredReferenceRecord, StoredRelationRecord, StoredSearchAfter, StoredSearchPlan,
    StoredSearchSymbol, StoredSymbolRecord,
};

pub struct StorageRetrievalAdapter<'a> {
    snapshot: &'a ReadSnapshot,
}

impl<'a> StorageRetrievalAdapter<'a> {
    #[must_use]
    pub const fn new(snapshot: &'a ReadSnapshot) -> Self {
        Self { snapshot }
    }
}

impl RetrievalStore for StorageRetrievalAdapter<'_> {
    type Error = StorageError;

    fn repository_id(&self) -> &ca_core::RepositoryId {
        self.snapshot.repository_id()
    }

    fn generation_id(&self) -> &ca_core::GenerationId {
        self.snapshot.generation_id()
    }

    fn coverage(&self) -> Result<CoverageSummary, Self::Error> {
        let coverage = self.snapshot.retrieval_coverage()?;
        Ok(CoverageSummary {
            status: coverage.status,
            warning_count: coverage.warning_count,
            unresolved_occurrences: coverage.unresolved_occurrences,
            resolver_version: coverage.resolver_version,
        })
    }

    fn search_symbols(&self, plan: &SearchPlan) -> Result<Vec<SearchRow>, Self::Error> {
        let stored = StoredSearchPlan {
            query: plan.query.clone(),
            folded_query: plan.folded_query.clone(),
            escaped_folded_prefix: plan.escaped_folded_prefix.clone(),
            fts_expression: plan.fts_expression.clone(),
            language: plan.language.clone(),
            kind: plan.kind.clone(),
            path_prefix: plan.path_prefix.clone(),
            after: plan.after.as_ref().map(stored_search_after),
            fetch_limit: plan.fetch_limit,
        };
        self.snapshot
            .retrieval_search_symbols(&stored)?
            .into_iter()
            .map(search_row)
            .collect()
    }

    fn symbol(&self, symbol_id: &str) -> Result<Option<SymbolRecord>, Self::Error> {
        self.snapshot
            .retrieval_symbol(symbol_id)?
            .map(symbol_record)
            .transpose()
    }

    fn file_symbols(
        &self,
        relative_path: &str,
        limit: usize,
    ) -> Result<Vec<SymbolRecord>, Self::Error> {
        self.snapshot
            .retrieval_file_symbols(relative_path, limit)?
            .into_iter()
            .map(symbol_record)
            .collect()
    }

    fn references_to(
        &self,
        symbol_id: &str,
        include_candidates: bool,
        after: Option<&ReferenceSortKey>,
        limit: usize,
    ) -> Result<Vec<ReferenceRecord>, Self::Error> {
        let after = after.map(|key| StoredReferenceAfter {
            relative_path: key.relative_path.clone(),
            start_byte: key.start_byte,
            edge_id: key.edge_id.clone(),
        });
        self.snapshot
            .retrieval_references_to(symbol_id, include_candidates, after.as_ref(), limit)?
            .into_iter()
            .map(reference_record)
            .collect()
    }

    fn adjacent_relations(
        &self,
        symbol_id: &str,
        direction: RelationDirection,
        filter: RelationFilter,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<RelationRecord>, Self::Error> {
        let direction = match direction {
            RelationDirection::Incoming => StoredGraphDirection::Incoming,
            RelationDirection::Outgoing => StoredGraphDirection::Outgoing,
        };
        let relationship = match filter {
            RelationFilter::Any => None,
            RelationFilter::Calls => Some("calls"),
            RelationFilter::References => Some("references"),
        };
        self.snapshot
            .retrieval_adjacent_relations(
                symbol_id,
                direction,
                relationship,
                include_candidates,
                limit,
            )?
            .into_iter()
            .map(relation_record)
            .collect()
    }

    fn incoming_relations_to_path(
        &self,
        relative_path: &str,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<RelationRecord>, Self::Error> {
        self.snapshot
            .retrieval_incoming_relations_to_path(relative_path, include_candidates, limit)?
            .into_iter()
            .map(relation_record)
            .collect()
    }

    fn repository_files(
        &self,
        path_prefix: Option<&str>,
        limit: usize,
    ) -> Result<Vec<IndexedFileRecord>, Self::Error> {
        Ok(self
            .snapshot
            .retrieval_repository_files(path_prefix, limit)?
            .into_iter()
            .map(indexed_file)
            .collect())
    }

    fn indexed_file(&self, relative_path: &str) -> Result<Option<IndexedFileRecord>, Self::Error> {
        Ok(self
            .snapshot
            .retrieval_indexed_file(relative_path)?
            .map(indexed_file))
    }
}

fn stored_search_after(key: &SearchSortKey) -> StoredSearchAfter {
    StoredSearchAfter {
        tier: key.tier,
        fts_score: key.fts_score,
        relative_path: key.relative_path.clone(),
        name: key.name.clone(),
        start_byte: key.start_byte,
        symbol_id: key.symbol_id.clone(),
    }
}

fn search_row(value: StoredSearchSymbol) -> Result<SearchRow, StorageError> {
    Ok(SearchRow {
        symbol: symbol_record(value.symbol)?,
        tier: value.tier,
        fts_score: value.fts_score,
    })
}

fn symbol_record(value: StoredSymbolRecord) -> Result<SymbolRecord, StorageError> {
    Ok(SymbolRecord {
        id: value.id,
        relative_path: value.relative_path,
        language: value.language,
        kind: value.kind,
        name: value.name,
        container: value.container,
        signature: value.signature,
        start_byte: value.start_byte,
        end_byte: value.end_byte,
        syntax_start_byte: value.syntax_start_byte,
        syntax_end_byte: value.syntax_end_byte,
        content_hash: value.content_hash,
        parse_status: value.parse_status,
        attributes: parse_string_list(&value.attributes_json, "symbol attributes")?,
        limitations: parse_string_list(&value.limitations_json, "symbol limitations")?,
    })
}

fn reference_record(value: StoredReferenceRecord) -> Result<ReferenceRecord, StorageError> {
    Ok(ReferenceRecord {
        edge_id: value.edge_id,
        source_observation_id: value.source_observation_id,
        relative_path: value.relative_path,
        spelling: value.spelling,
        start_byte: value.start_byte,
        end_byte: value.end_byte,
        resolution: value.resolution,
        candidate_count: value.candidate_count,
        rule_version: value.rule_version,
        limitations: parse_string_list(&value.limitations_json, "edge limitations")?,
    })
}

fn relation_record(value: StoredRelationRecord) -> Result<RelationRecord, StorageError> {
    Ok(RelationRecord {
        id: value.id,
        relationship: value.relationship,
        source_symbol_id: value.source_symbol_id,
        source_observation_id: value.source_observation_id,
        source_path: value.source_path,
        source_start_byte: value.source_start_byte,
        target_symbol_id: value.target_symbol_id,
        target_path: value.target_path,
        resolution: value.resolution,
        candidate_count: value.candidate_count,
        rule_version: value.rule_version,
        reason: value.reason,
        limitations: parse_string_list(&value.limitations_json, "edge limitations")?,
    })
}

fn indexed_file(value: StoredIndexedFile) -> IndexedFileRecord {
    IndexedFileRecord {
        relative_path: value.relative_path,
        content_hash: value.content_hash,
        language: value.language,
        parse_status: value.parse_status,
        coverage: value.coverage_json,
        byte_length: value.byte_length,
        symbol_count: value.symbol_count,
    }
}

fn parse_string_list(value: &str, field: &str) -> Result<Vec<String>, StorageError> {
    serde_json::from_str(value).map_err(|error| {
        StorageError::InvalidInput(format!("stored {field} is not a string array: {error}"))
    })
}
