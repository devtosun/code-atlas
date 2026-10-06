use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    error::Error,
    path::{Component, Path, PathBuf},
};

use ca_core::{ByteRange, CancellationContext, GenerationId};
use thiserror::Error;

pub const RESOLVER_VERSION: &str = "codeatlas-resolver-v2";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationKind {
    Symbol,
    Scope,
    Import,
    Reference,
    CallSite,
}

impl ObservationKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Scope => "scope",
            Self::Import => "import",
            Self::Reference => "reference",
            Self::CallSite => "call_site",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionObservation {
    pub category: ObservationKind,
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
    pub attributes: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionFile {
    pub relative_path: String,
    pub language: String,
    pub observations: Vec<ResolutionObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestData {
    pub relative_path: String,
    pub contents: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionInput {
    pub generation_id: GenerationId,
    pub files: Vec<ResolutionFile>,
    pub manifests: Vec<ManifestData>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RelationshipKind {
    Contains,
    Imports,
    References,
    Calls,
}

impl RelationshipKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Imports => "imports",
            Self::References => "references",
            Self::Calls => "calls",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResolutionLabel {
    SyntaxObservation,
    LexicallyResolved,
    Candidate,
    Unresolved,
}

impl ResolutionLabel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SyntaxObservation => "syntax_observation",
            Self::LexicallyResolved => "lexically_resolved",
            Self::Candidate => "candidate",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalSymbolRef {
    pub relative_path: String,
    pub symbol_id: String,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EdgeTarget {
    LocalFile { relative_path: String },
    LocalSymbol(LocalSymbolRef),
    External { node_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEdge {
    pub id: String,
    pub relationship: RelationshipKind,
    pub source_path: String,
    pub source_observation_id: String,
    pub source_category: ObservationKind,
    pub source_range: ByteRange,
    pub source_symbol: Option<LocalSymbolRef>,
    pub target: Option<EdgeTarget>,
    pub resolution: ResolutionLabel,
    pub rule: String,
    pub resolver_version: String,
    pub candidate_count: usize,
    pub reason: String,
    pub evidence: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalNode {
    pub id: String,
    pub language: String,
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionDiagnostic {
    pub relative_path: String,
    pub observation_id: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolutionStats {
    pub resolved: usize,
    pub candidates: usize,
    pub unresolved: usize,
    pub external: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionGraph {
    pub generation_id: GenerationId,
    pub resolver_version: String,
    pub edges: Vec<ResolvedEdge>,
    pub external_nodes: Vec<ExternalNode>,
    pub diagnostics: Vec<ResolutionDiagnostic>,
    pub stats: ResolutionStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolutionLimits {
    pub max_import_depth: usize,
    pub max_import_nodes: usize,
    pub max_import_edges: usize,
    pub max_candidates_per_site: usize,
    pub max_diagnostics: usize,
}

impl Default for ResolutionLimits {
    fn default() -> Self {
        Self {
            max_import_depth: 16,
            max_import_nodes: 1_024,
            max_import_edges: 4_096,
            max_candidates_per_site: 32,
            max_diagnostics: 1_024,
        }
    }
}

impl ResolutionLimits {
    fn validate(self) -> Result<(), ResolutionError> {
        if self.max_import_depth == 0
            || self.max_import_depth > 64
            || self.max_import_nodes == 0
            || self.max_import_nodes > 100_000
            || self.max_import_edges == 0
            || self.max_import_edges > 1_000_000
            || self.max_candidates_per_site == 0
            || self.max_candidates_per_site > 1_024
            || self.max_diagnostics == 0
            || self.max_diagnostics > 100_000
        {
            return Err(ResolutionError::InvalidLimits);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ResolutionError {
    #[error("invalid resolution limits")]
    InvalidLimits,
    #[error("resolution cancelled")]
    Cancelled,
    #[error("invalid or over-budget lexical scope chain")]
    InvalidScopeChain,
}

#[derive(Clone, Debug)]
struct Candidate {
    file_index: usize,
    observation_index: usize,
    rule: &'static str,
    evidence: Vec<String>,
}

#[derive(Clone, Debug)]
enum ImportDestination {
    Local {
        files: Vec<usize>,
        rule: &'static str,
    },
    External {
        kind: &'static str,
        rule: &'static str,
    },
    Unresolved {
        code: &'static str,
        reason: String,
    },
}

struct Resolver<'a> {
    input: &'a ResolutionInput,
    limits: ResolutionLimits,
    cancellation: &'a CancellationContext,
    path_index: BTreeMap<String, usize>,
    manifests: ManifestIndex,
    edges: Vec<ResolvedEdge>,
    external_nodes: BTreeMap<String, ExternalNode>,
    diagnostics: Vec<ResolutionDiagnostic>,
    stats: ResolutionStats,
}

#[derive(Default)]
struct ManifestIndex {
    go_modules: Vec<(String, String)>,
    dart_packages: Vec<(String, String)>,
}

pub fn resolve_generation(
    input: &ResolutionInput,
    limits: ResolutionLimits,
    cancellation: &CancellationContext,
) -> Result<ResolutionGraph, ResolutionError> {
    limits.validate()?;
    let mut path_index = BTreeMap::new();
    for (index, file) in input.files.iter().enumerate() {
        path_index.insert(file.relative_path.clone(), index);
    }
    let mut resolver = Resolver {
        input,
        limits,
        cancellation,
        path_index,
        manifests: parse_manifests(&input.manifests),
        edges: Vec::new(),
        external_nodes: BTreeMap::new(),
        diagnostics: Vec::new(),
        stats: ResolutionStats::default(),
    };
    resolver.resolve()?;
    resolver.edges.sort_by(|left, right| left.id.cmp(&right.id));
    resolver.edges.dedup_by(|left, right| left.id == right.id);
    resolver.diagnostics.sort_by(|left, right| {
        (
            left.relative_path.as_str(),
            left.observation_id.as_deref(),
            left.code.as_str(),
            left.message.as_str(),
        )
            .cmp(&(
                right.relative_path.as_str(),
                right.observation_id.as_deref(),
                right.code.as_str(),
                right.message.as_str(),
            ))
    });
    resolver.diagnostics.dedup();
    resolver.diagnostics.truncate(limits.max_diagnostics);
    Ok(ResolutionGraph {
        generation_id: input.generation_id.clone(),
        resolver_version: RESOLVER_VERSION.to_owned(),
        edges: resolver.edges,
        external_nodes: resolver.external_nodes.into_values().collect(),
        diagnostics: resolver.diagnostics,
        stats: resolver.stats,
    })
}

impl Resolver<'_> {
    fn resolve(&mut self) -> Result<(), ResolutionError> {
        for file_index in 0..self.input.files.len() {
            self.check_cancelled()?;
            self.resolve_containment(file_index);
            self.resolve_import_edges(file_index)?;
            self.resolve_occurrences(file_index, ObservationKind::Reference)?;
            self.resolve_occurrences(file_index, ObservationKind::CallSite)?;
        }
        Ok(())
    }

    fn check_cancelled(&self) -> Result<(), ResolutionError> {
        if self.cancellation.is_cancelled() {
            Err(ResolutionError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn resolve_containment(&mut self, file_index: usize) {
        let file = &self.input.files[file_index];
        let symbols = file
            .observations
            .iter()
            .enumerate()
            .filter(|(_, observation)| observation.category == ObservationKind::Symbol)
            .collect::<Vec<_>>();
        for (child_index, child) in &symbols {
            let container = symbols
                .iter()
                .filter(|(container_index, container)| {
                    container_index != child_index
                        && contains(container.syntax_range, child.byte_range)
                        && is_container_kind(&container.kind)
                })
                .min_by_key(|(_, container)| range_length(container.syntax_range));
            if let Some((_, container)) = container {
                let child_ref = LocalSymbolRef {
                    relative_path: file.relative_path.clone(),
                    symbol_id: child.id.clone(),
                };
                self.push_edge(
                    RelationshipKind::Contains,
                    file,
                    container,
                    Some(LocalSymbolRef {
                        relative_path: file.relative_path.clone(),
                        symbol_id: container.id.clone(),
                    }),
                    Some(EdgeTarget::LocalSymbol(child_ref)),
                    ResolutionLabel::LexicallyResolved,
                    "lexical-containment-v1",
                    1,
                    "this declaration is the smallest enclosing declaration for the target",
                    vec![format!(
                        "child_range:{}..{}",
                        child.syntax_range.start(),
                        child.syntax_range.end()
                    )],
                    Vec::new(),
                );
            }
        }
    }

    fn resolve_import_edges(&mut self, file_index: usize) -> Result<(), ResolutionError> {
        let file = &self.input.files[file_index];
        for import in file.observations.iter().filter(|observation| {
            observation.category == ObservationKind::Import
                && !is_module_declaration_observation(observation)
                && !is_local_export_observation(observation)
        }) {
            self.check_cancelled()?;
            match self.import_destination(file_index, import) {
                ImportDestination::Local { mut files, rule } => {
                    files.sort_unstable();
                    files.dedup();
                    let targets = self.import_symbol_targets(file_index, import, &files)?;
                    if targets.is_empty() {
                        let count = files.len();
                        for target_index in files.iter().take(self.limits.max_candidates_per_site) {
                            let target = &self.input.files[*target_index];
                            self.push_edge(
                                RelationshipKind::Imports,
                                file,
                                import,
                                self.enclosing_symbol(file_index, import),
                                Some(EdgeTarget::LocalFile {
                                    relative_path: target.relative_path.clone(),
                                }),
                                if count == 1 {
                                    ResolutionLabel::LexicallyResolved
                                } else {
                                    ResolutionLabel::Candidate
                                },
                                rule,
                                count,
                                if count == 1 {
                                    "the module rule selected one indexed file"
                                } else {
                                    "the module rule produced multiple indexed files"
                                },
                                vec![format!("specifier:{}", import.spelling)],
                                import.limitations.clone(),
                            );
                        }
                    } else {
                        self.emit_candidates(
                            RelationshipKind::Imports,
                            file_index,
                            import,
                            targets,
                            "an explicit import or re-export binding selected indexed declarations",
                        );
                    }
                }
                ImportDestination::External { kind, rule } => {
                    let node_id = external_node_id(&file.language, kind, &import.spelling);
                    self.external_nodes
                        .entry(node_id.clone())
                        .or_insert_with(|| ExternalNode {
                            id: node_id.clone(),
                            language: file.language.clone(),
                            kind: kind.to_owned(),
                            label: import.spelling.clone(),
                        });
                    self.push_edge(
                        RelationshipKind::Imports,
                        file,
                        import,
                        self.enclosing_symbol(file_index, import),
                        Some(EdgeTarget::External { node_id }),
                        ResolutionLabel::SyntaxObservation,
                        rule,
                        1,
                        "the static specifier is outside the indexed local module rules",
                        vec![format!("specifier:{}", import.spelling)],
                        import.limitations.clone(),
                    );
                    self.stats.external = self.stats.external.saturating_add(1);
                }
                ImportDestination::Unresolved { code, reason } => {
                    self.push_edge(
                        RelationshipKind::Imports,
                        file,
                        import,
                        self.enclosing_symbol(file_index, import),
                        None,
                        ResolutionLabel::Unresolved,
                        "unsupported-import-v1",
                        0,
                        &reason,
                        vec![format!("specifier:{}", import.spelling)],
                        import.limitations.clone(),
                    );
                    self.push_diagnostic(file, Some(import), code, reason);
                }
            }
        }
        Ok(())
    }

    fn resolve_occurrences(
        &mut self,
        file_index: usize,
        category: ObservationKind,
    ) -> Result<(), ResolutionError> {
        let observations = self.input.files[file_index]
            .observations
            .iter()
            .filter(|observation| observation.category == category)
            .cloned()
            .collect::<Vec<_>>();
        for occurrence in observations {
            self.check_cancelled()?;
            let relationship = if category == ObservationKind::CallSite {
                RelationshipKind::Calls
            } else {
                RelationshipKind::References
            };
            if is_dynamic(&occurrence) {
                let file = &self.input.files[file_index];
                self.push_edge(
                    relationship,
                    file,
                    &occurrence,
                    self.enclosing_symbol(file_index, &occurrence),
                    None,
                    ResolutionLabel::Unresolved,
                    "dynamic-target-v1",
                    0,
                    "the target uses dynamic, computed, macro, or explicitly non-call syntax",
                    Vec::new(),
                    occurrence.limitations.clone(),
                );
                continue;
            }

            let mut candidates = self.lexical_candidates(file_index, &occurrence, category)?;
            if candidates.is_empty() {
                candidates = self.imported_candidates(file_index, &occurrence, category)?;
            }
            if candidates.is_empty() {
                candidates = self.same_module_candidates(file_index, &occurrence, category);
            }
            if has_uncertain_receiver(&occurrence) {
                for candidate in &mut candidates {
                    candidate.rule = "receiver-name-candidate-v1";
                    candidate
                        .evidence
                        .push("receiver_type_not_inferred".to_owned());
                }
                let file = &self.input.files[file_index];
                if candidates.is_empty() {
                    self.push_edge(
                        relationship,
                        file,
                        &occurrence,
                        self.enclosing_symbol(file_index, &occurrence),
                        None,
                        ResolutionLabel::Unresolved,
                        "receiver-uncertainty-v1",
                        0,
                        "a receiver expression is not a proven static type or import prefix",
                        Vec::new(),
                        occurrence.limitations.clone(),
                    );
                } else {
                    self.emit_candidates(
                        relationship,
                        file_index,
                        &occurrence,
                        candidates,
                        "matching member names are candidates because receiver dispatch is not inferred",
                    );
                }
            } else if candidates.is_empty() {
                let file = &self.input.files[file_index];
                self.push_edge(
                    relationship,
                    file,
                    &occurrence,
                    self.enclosing_symbol(file_index, &occurrence),
                    None,
                    ResolutionLabel::Unresolved,
                    "no-supported-binding-v1",
                    0,
                    "no supported lexical, module, or import rule selected a declaration",
                    Vec::new(),
                    occurrence.limitations.clone(),
                );
            } else {
                self.emit_candidates(
                    relationship,
                    file_index,
                    &occurrence,
                    candidates,
                    "bounded lexical/module rules selected declaration evidence",
                );
            }
        }
        Ok(())
    }

    fn emit_candidates(
        &mut self,
        relationship: RelationshipKind,
        file_index: usize,
        source: &ResolutionObservation,
        mut candidates: Vec<Candidate>,
        reason: &str,
    ) {
        candidates.sort_by(|left, right| {
            let left_file = &self.input.files[left.file_index];
            let right_file = &self.input.files[right.file_index];
            let left_symbol = &left_file.observations[left.observation_index];
            let right_symbol = &right_file.observations[right.observation_index];
            (
                left_file.relative_path.as_str(),
                left_symbol.byte_range.start(),
                left_symbol.id.as_str(),
            )
                .cmp(&(
                    right_file.relative_path.as_str(),
                    right_symbol.byte_range.start(),
                    right_symbol.id.as_str(),
                ))
        });
        candidates.dedup_by(|left, right| {
            let left_symbol =
                &self.input.files[left.file_index].observations[left.observation_index];
            let right_symbol =
                &self.input.files[right.file_index].observations[right.observation_index];
            left.file_index == right.file_index && left_symbol.id == right_symbol.id
        });
        let count = candidates.len();
        let force_candidate = candidates
            .iter()
            .any(|candidate| candidate.rule == "receiver-name-candidate-v1");
        let label = if count == 1 && !force_candidate {
            ResolutionLabel::LexicallyResolved
        } else {
            ResolutionLabel::Candidate
        };
        let source_file = &self.input.files[file_index];
        let source_symbol = self.enclosing_symbol(file_index, source);
        for candidate in candidates.iter().take(self.limits.max_candidates_per_site) {
            let target_file = &self.input.files[candidate.file_index];
            let target = &target_file.observations[candidate.observation_index];
            let mut limitations = source.limitations.clone();
            if count > 1 {
                limitations.push("ambiguous_candidate_set".to_owned());
            }
            self.push_edge(
                relationship,
                source_file,
                source,
                source_symbol.clone(),
                Some(EdgeTarget::LocalSymbol(LocalSymbolRef {
                    relative_path: target_file.relative_path.clone(),
                    symbol_id: target.id.clone(),
                })),
                label,
                candidate.rule,
                count,
                reason,
                candidate.evidence.clone(),
                limitations,
            );
        }
        if count > self.limits.max_candidates_per_site {
            self.push_diagnostic(
                source_file,
                Some(source),
                "candidate_limit_reached",
                format!(
                    "{} candidates were found; only {} evidence edges were retained",
                    count, self.limits.max_candidates_per_site
                ),
            );
        }
    }

    fn lexical_candidates(
        &self,
        file_index: usize,
        occurrence: &ResolutionObservation,
        category: ObservationKind,
    ) -> Result<Vec<Candidate>, ResolutionError> {
        if has_uncertain_receiver(occurrence) {
            return Ok(self.member_name_candidates(file_index, occurrence, category));
        }
        let file = &self.input.files[file_index];
        let name = occurrence_name(occurrence);
        let chain = scope_chain(file, occurrence.scope_id.as_deref(), self.cancellation)?;
        let mut ranked = Vec::new();
        for (observation_index, symbol) in file.observations.iter().enumerate() {
            if symbol.category != ObservationKind::Symbol
                || symbol.spelling != name
                || !role_compatible(occurrence, symbol)
                || (category == ObservationKind::CallSite && !is_callable_kind(&symbol.kind))
                || (is_order_sensitive(&symbol.kind)
                    && symbol.byte_range.start() > occurrence.byte_range.start())
            {
                continue;
            }
            let Some(distance) = chain
                .iter()
                .position(|scope| symbol.scope_id.as_deref() == Some(scope.as_str()))
            else {
                continue;
            };
            ranked.push((distance, observation_index));
        }
        let Some(best) = ranked.iter().map(|(distance, _)| *distance).min() else {
            return Ok(Vec::new());
        };
        Ok(ranked
            .into_iter()
            .filter(|(distance, _)| *distance == best)
            .map(|(_, observation_index)| Candidate {
                file_index,
                observation_index,
                rule: "lexical-scope-v1",
                evidence: vec![format!("scope_distance:{best}"), format!("name:{name}")],
            })
            .collect())
    }

    fn member_name_candidates(
        &self,
        file_index: usize,
        occurrence: &ResolutionObservation,
        category: ObservationKind,
    ) -> Vec<Candidate> {
        let name = occurrence_name(occurrence);
        self.input.files[file_index]
            .observations
            .iter()
            .enumerate()
            .filter(|(_, symbol)| {
                symbol.category == ObservationKind::Symbol
                    && symbol.spelling == name
                    && role_compatible(occurrence, symbol)
                    && (category != ObservationKind::CallSite || is_callable_kind(&symbol.kind))
                    && is_member_kind(&symbol.kind)
            })
            .map(|(observation_index, _)| Candidate {
                file_index,
                observation_index,
                rule: "receiver-name-candidate-v1",
                evidence: vec![format!("member_name:{name}")],
            })
            .collect()
    }

    fn imported_candidates(
        &mut self,
        file_index: usize,
        occurrence: &ResolutionObservation,
        category: ObservationKind,
    ) -> Result<Vec<Candidate>, ResolutionError> {
        let file = &self.input.files[file_index];
        let (parsed_qualifier, name) = split_qualified_name(&occurrence.spelling);
        let qualifier = parsed_qualifier.or_else(|| occurrence.receiver.clone());
        let chain = scope_chain(file, occurrence.scope_id.as_deref(), self.cancellation)?;
        let matching = file
            .observations
            .iter()
            .filter_map(|import| {
                if import.category != ObservationKind::Import
                    || is_module_declaration_observation(import)
                    || is_local_export_observation(import)
                {
                    return None;
                }
                let distance = chain
                    .iter()
                    .position(|scope| import.scope_id.as_deref() == Some(scope))?;
                let name =
                    import_matches_occurrence(&file.language, import, qualifier.as_deref(), &name)?;
                Some((distance, import, name))
            })
            .collect::<Vec<_>>();
        let best = matching.iter().map(|(distance, _, _)| *distance).min();
        let mut candidates = Vec::new();
        for (_, import, imported_name) in matching
            .into_iter()
            .filter(|(distance, _, _)| Some(*distance) == best)
        {
            self.check_cancelled()?;
            let ImportDestination::Local { files, rule } =
                self.import_destination(file_index, import)
            else {
                continue;
            };
            candidates.extend(self.symbol_targets_for_name(
                &files,
                &imported_name,
                category,
                occurrence,
                rule,
            )?);
        }
        Ok(candidates)
    }

    fn same_module_candidates(
        &self,
        file_index: usize,
        occurrence: &ResolutionObservation,
        category: ObservationKind,
    ) -> Vec<Candidate> {
        let file = &self.input.files[file_index];
        if !matches!(file.language.as_str(), "go" | "java" | "csharp") {
            return Vec::new();
        }
        let module = declared_module(file);
        let Some(module) = module else {
            return Vec::new();
        };
        let name = occurrence_name(occurrence);
        let mut candidates = Vec::new();
        for (target_file_index, target_file) in self.input.files.iter().enumerate() {
            if target_file_index == file_index
                || target_file.language != file.language
                || declared_module(target_file).as_deref() != Some(module.as_str())
                || (file.language == "go"
                    && Path::new(&target_file.relative_path).parent()
                        != Path::new(&file.relative_path).parent())
            {
                continue;
            }
            for (observation_index, symbol) in target_file.observations.iter().enumerate() {
                if symbol.category == ObservationKind::Symbol
                    && symbol.spelling == name
                    && role_compatible(occurrence, symbol)
                    && (category != ObservationKind::CallSite || is_callable_kind(&symbol.kind))
                    && if has_uncertain_receiver(occurrence) {
                        is_member_kind(&symbol.kind)
                    } else {
                        is_module_level_symbol(target_file, symbol)
                    }
                {
                    candidates.push(Candidate {
                        file_index: target_file_index,
                        observation_index,
                        rule: "same-module-v1",
                        evidence: vec![format!("module:{module}")],
                    });
                }
            }
        }
        candidates
    }

    fn import_symbol_targets(
        &mut self,
        source_file_index: usize,
        import: &ResolutionObservation,
        files: &[usize],
    ) -> Result<Vec<Candidate>, ResolutionError> {
        let source_file = &self.input.files[source_file_index];
        let imported_name = imported_binding_name(&source_file.language, import);
        let Some(imported_name) = imported_name else {
            return Ok(Vec::new());
        };
        self.symbol_targets_for_name(
            files,
            &imported_name,
            ObservationKind::Reference,
            import,
            "explicit-import-v1",
        )
    }

    fn symbol_targets_for_name(
        &mut self,
        files: &[usize],
        name: &str,
        category: ObservationKind,
        occurrence: &ResolutionObservation,
        rule: &'static str,
    ) -> Result<Vec<Candidate>, ResolutionError> {
        let mut output = Vec::new();
        let mut visited = HashSet::new();
        let mut budget = TraversalBudget::default();
        for file_index in files {
            self.collect_exported_targets(
                *file_index,
                name,
                category,
                occurrence,
                rule,
                0,
                &mut visited,
                &mut budget,
                &mut output,
            )?;
        }
        Ok(output)
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_exported_targets(
        &mut self,
        file_index: usize,
        name: &str,
        category: ObservationKind,
        occurrence: &ResolutionObservation,
        rule: &'static str,
        depth: usize,
        visited: &mut HashSet<(usize, String)>,
        budget: &mut TraversalBudget,
        output: &mut Vec<Candidate>,
    ) -> Result<(), ResolutionError> {
        self.check_cancelled()?;
        if depth > self.limits.max_import_depth {
            let file = &self.input.files[file_index];
            self.push_diagnostic(
                file,
                Some(occurrence),
                "import_depth_limit_reached",
                format!(
                    "import traversal exceeded depth {}",
                    self.limits.max_import_depth
                ),
            );
            return Ok(());
        }
        if !visited.insert((file_index, name.to_owned())) {
            let file = &self.input.files[file_index];
            self.push_diagnostic(
                file,
                Some(occurrence),
                "import_cycle_detected",
                format!("cycle detected while resolving export '{name}'"),
            );
            return Ok(());
        }
        budget.nodes = budget.nodes.saturating_add(1);
        if budget.nodes > self.limits.max_import_nodes {
            let file = &self.input.files[file_index];
            self.push_diagnostic(
                file,
                Some(occurrence),
                "import_node_limit_reached",
                format!(
                    "import traversal exceeded {} nodes",
                    self.limits.max_import_nodes
                ),
            );
            visited.remove(&(file_index, name.to_owned()));
            return Ok(());
        }
        let file = &self.input.files[file_index];
        for (observation_index, symbol) in file.observations.iter().enumerate() {
            if symbol.category == ObservationKind::Symbol
                && symbol.spelling == name
                && role_compatible(occurrence, symbol)
                && (category != ObservationKind::CallSite || is_callable_kind(&symbol.kind))
                && is_module_visible(&file.language, symbol)
                && (file.language == "rust"
                    || is_module_level_symbol(file, symbol)
                    || occurrence
                        .attributes
                        .iter()
                        .any(|attribute| attribute == "static_import"))
            {
                output.push(Candidate {
                    file_index,
                    observation_index,
                    rule,
                    evidence: vec![
                        format!("export_name:{name}"),
                        format!("module:{}", file.relative_path),
                    ],
                });
            }
        }
        let reexports = file
            .observations
            .iter()
            .filter(|item| {
                item.category == ObservationKind::Import
                    && matches!(item.kind.as_str(), "reexport" | "export")
            })
            .cloned()
            .collect::<Vec<_>>();
        for reexport in reexports {
            if file.language == "dart" && !dart_combinators_allow(&reexport, name) {
                continue;
            }
            let exported_name = reexport.alias.as_deref();
            if exported_name.is_some_and(|alias| alias != name) {
                continue;
            }
            let local_name = attribute_value(&reexport.attributes, "local:").unwrap_or_else(|| {
                if reexport.kind == "export" && file.language != "dart" {
                    reexport.spelling.clone()
                } else {
                    name.to_owned()
                }
            });
            if reexport.kind == "export" && file.language != "dart" {
                if reexport.alias.is_none() && reexport.spelling != name {
                    continue;
                }
                for (observation_index, symbol) in file.observations.iter().enumerate() {
                    if symbol.category == ObservationKind::Symbol
                        && symbol.spelling == local_name
                        && role_compatible(occurrence, symbol)
                        && is_module_level_symbol(file, symbol)
                        && (category != ObservationKind::CallSite || is_callable_kind(&symbol.kind))
                    {
                        output.push(Candidate {
                            file_index,
                            observation_index,
                            rule: "local-export-v2",
                            evidence: vec![
                                format!("export_name:{name}"),
                                format!("local_name:{local_name}"),
                            ],
                        });
                    }
                }
                continue;
            }
            let ImportDestination::Local { files, .. } =
                self.import_destination(file_index, &reexport)
            else {
                continue;
            };
            for target_file in files {
                budget.edges = budget.edges.saturating_add(1);
                if budget.edges > self.limits.max_import_edges {
                    self.push_diagnostic(
                        file,
                        Some(occurrence),
                        "import_edge_limit_reached",
                        format!(
                            "import traversal exceeded {} edges",
                            self.limits.max_import_edges
                        ),
                    );
                    visited.remove(&(file_index, name.to_owned()));
                    return Ok(());
                }
                self.collect_exported_targets(
                    target_file,
                    &local_name,
                    category,
                    occurrence,
                    "bounded-reexport-v1",
                    depth.saturating_add(1),
                    visited,
                    budget,
                    output,
                )?;
            }
        }
        visited.remove(&(file_index, name.to_owned()));
        Ok(())
    }

    fn import_destination(
        &self,
        source_file_index: usize,
        import: &ResolutionObservation,
    ) -> ImportDestination {
        let source = &self.input.files[source_file_index];
        if import
            .attributes
            .iter()
            .any(|item| item == "dynamic_specifier")
            || import.spelling == "<missing>"
        {
            return ImportDestination::Unresolved {
                code: "dynamic_module_specifier",
                reason: "dynamic module specifiers are retained without a guessed target"
                    .to_owned(),
            };
        }
        match source.language.as_str() {
            "javascript" | "jsx" | "typescript" | "tsx" => {
                if !import.spelling.starts_with('.') {
                    return ImportDestination::External {
                        kind: "package",
                        rule: "ecmascript-external-package-v1",
                    };
                }
                let files = resolve_relative_module(
                    &source.relative_path,
                    &import.spelling,
                    &self.path_index,
                    &["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "d.ts"],
                );
                local_or_missing(files, "ecmascript-relative-module-v1", &import.spelling)
            }
            "dart" => {
                if import.spelling.starts_with("dart:") {
                    return ImportDestination::External {
                        kind: "sdk_library",
                        rule: "dart-sdk-library-v1",
                    };
                }
                if let Some(rest) = import.spelling.strip_prefix("package:") {
                    let Some((package, path)) = rest.split_once('/') else {
                        return ImportDestination::Unresolved {
                            code: "invalid_package_uri",
                            reason: "the Dart package URI has no library path".to_owned(),
                        };
                    };
                    if let Some((manifest_dir, _)) = self
                        .manifests
                        .dart_packages
                        .iter()
                        .find(|(_, name)| name == package)
                    {
                        let candidate = join_normalized(manifest_dir, &format!("lib/{path}"));
                        let files = candidate
                            .and_then(|candidate| self.path_index.get(&candidate).copied())
                            .into_iter()
                            .collect();
                        return local_or_missing(
                            files,
                            "dart-package-manifest-v1",
                            &import.spelling,
                        );
                    }
                    return ImportDestination::External {
                        kind: "package",
                        rule: "dart-external-package-v1",
                    };
                }
                let files = resolve_relative_module(
                    &source.relative_path,
                    &import.spelling,
                    &self.path_index,
                    &["dart"],
                );
                local_or_missing(files, "dart-relative-library-v1", &import.spelling)
            }
            "rust" => {
                let mut files =
                    resolve_rust_module(&source.relative_path, &import.spelling, &self.path_index);
                if files.is_empty() && rust_inline_module_matches(source, &import.spelling) {
                    files.push(source_file_index);
                }
                if files.is_empty() && !starts_with_local_rust_root(&import.spelling) {
                    ImportDestination::External {
                        kind: "crate",
                        rule: "rust-external-crate-v1",
                    }
                } else {
                    local_or_missing(files, "rust-module-path-v1", &import.spelling)
                }
            }
            "go" => {
                if let Some((manifest_dir, module)) =
                    self.manifests.go_modules.iter().find(|(_, module)| {
                        import.spelling == **module
                            || import
                                .spelling
                                .strip_prefix(module.as_str())
                                .is_some_and(|rest| rest.starts_with('/'))
                    })
                {
                    let suffix = import
                        .spelling
                        .strip_prefix(module.as_str())
                        .unwrap_or("")
                        .trim_start_matches('/');
                    let directory = join_normalized(manifest_dir, suffix)
                        .unwrap_or_else(|| manifest_dir.clone());
                    let files = self
                        .input
                        .files
                        .iter()
                        .enumerate()
                        .filter(|(_, file)| {
                            file.language == "go" && parent_path(&file.relative_path) == directory
                        })
                        .map(|(index, _)| index)
                        .collect::<Vec<_>>();
                    return local_or_missing(files, "go-module-manifest-v1", &import.spelling);
                }
                ImportDestination::External {
                    kind: "go_package",
                    rule: "go-external-package-v1",
                }
            }
            "java" => {
                let package = if import
                    .attributes
                    .iter()
                    .any(|item| item == "wildcard_import")
                {
                    import.spelling.clone()
                } else {
                    import
                        .spelling
                        .rsplit_once('.')
                        .map_or_else(String::new, |(package, _)| package.to_owned())
                };
                let files = self.files_for_declared_module("java", &package);
                if files.is_empty() {
                    ImportDestination::External {
                        kind: "java_package",
                        rule: "java-external-classpath-v1",
                    }
                } else {
                    ImportDestination::Local {
                        files,
                        rule: "java-declared-package-v1",
                    }
                }
            }
            "csharp" => {
                let namespace = import.spelling.trim_end_matches(".*");
                let files = self.files_for_declared_module("csharp", namespace);
                if files.is_empty() {
                    ImportDestination::External {
                        kind: "dotnet_namespace",
                        rule: "csharp-external-namespace-v1",
                    }
                } else {
                    ImportDestination::Local {
                        files,
                        rule: "csharp-declared-namespace-v1",
                    }
                }
            }
            _ => ImportDestination::Unresolved {
                code: "unsupported_language_rule",
                reason: format!("no module rule exists for language '{}'", source.language),
            },
        }
    }

    fn files_for_declared_module(&self, language: &str, module: &str) -> Vec<usize> {
        self.input
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| {
                file.language == language && declared_module(file).as_deref() == Some(module)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn enclosing_symbol(
        &self,
        file_index: usize,
        observation: &ResolutionObservation,
    ) -> Option<LocalSymbolRef> {
        let file = &self.input.files[file_index];
        file.observations
            .iter()
            .filter(|candidate| {
                candidate.category == ObservationKind::Symbol
                    && is_callable_container(&candidate.kind)
                    && contains(candidate.syntax_range, observation.byte_range)
            })
            .min_by_key(|candidate| range_length(candidate.syntax_range))
            .map(|symbol| LocalSymbolRef {
                relative_path: file.relative_path.clone(),
                symbol_id: symbol.id.clone(),
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn push_edge(
        &mut self,
        relationship: RelationshipKind,
        source_file: &ResolutionFile,
        source: &ResolutionObservation,
        source_symbol: Option<LocalSymbolRef>,
        target: Option<EdgeTarget>,
        resolution: ResolutionLabel,
        rule: &str,
        candidate_count: usize,
        reason: &str,
        evidence: Vec<String>,
        limitations: Vec<String>,
    ) {
        let id = edge_id(
            relationship,
            &source_file.relative_path,
            &source.id,
            target.as_ref(),
            resolution,
            rule,
        );
        match resolution {
            ResolutionLabel::LexicallyResolved => {
                self.stats.resolved = self.stats.resolved.saturating_add(1);
            }
            ResolutionLabel::Candidate => {
                self.stats.candidates = self.stats.candidates.saturating_add(1);
            }
            ResolutionLabel::Unresolved => {
                self.stats.unresolved = self.stats.unresolved.saturating_add(1);
            }
            ResolutionLabel::SyntaxObservation => {}
        }
        self.edges.push(ResolvedEdge {
            id,
            relationship,
            source_path: source_file.relative_path.clone(),
            source_observation_id: source.id.clone(),
            source_category: source.category,
            source_range: source.byte_range,
            source_symbol,
            target,
            resolution,
            rule: rule.to_owned(),
            resolver_version: RESOLVER_VERSION.to_owned(),
            candidate_count,
            reason: reason.to_owned(),
            evidence,
            limitations,
        });
    }

    fn push_diagnostic(
        &mut self,
        file: &ResolutionFile,
        observation: Option<&ResolutionObservation>,
        code: &str,
        message: impl Into<String>,
    ) {
        if self.diagnostics.len() < self.limits.max_diagnostics {
            self.diagnostics.push(ResolutionDiagnostic {
                relative_path: file.relative_path.clone(),
                observation_id: observation.map(|item| item.id.clone()),
                code: code.to_owned(),
                message: message.into(),
            });
        }
    }
}

#[derive(Default)]
struct TraversalBudget {
    nodes: usize,
    edges: usize,
}

fn parse_manifests(manifests: &[ManifestData]) -> ManifestIndex {
    let mut output = ManifestIndex::default();
    for manifest in manifests {
        let directory = parent_path(&manifest.relative_path);
        if manifest.relative_path.ends_with("go.mod") {
            if let Some(module) = manifest.contents.lines().find_map(|line| {
                let line = line.trim();
                line.strip_prefix("module ")
                    .map(str::trim)
                    .filter(|value| !value.is_empty() && !value.contains(char::is_whitespace))
            }) {
                output.go_modules.push((directory, module.to_owned()));
            }
        } else if manifest.relative_path.ends_with("pubspec.yaml")
            && let Some(name) = manifest.contents.lines().find_map(|line| {
                let line = line.trim();
                line.strip_prefix("name:")
                    .map(str::trim)
                    .map(|value| value.trim_matches(['\'', '"']))
                    .filter(|value| !value.is_empty() && !value.contains(char::is_whitespace))
            })
        {
            output.dart_packages.push((directory, name.to_owned()));
        }
    }
    output
}

fn local_or_missing(files: Vec<usize>, rule: &'static str, specifier: &str) -> ImportDestination {
    if files.is_empty() {
        ImportDestination::Unresolved {
            code: "local_module_not_found",
            reason: format!("local module specifier '{specifier}' did not match an indexed file"),
        }
    } else {
        ImportDestination::Local { files, rule }
    }
}

fn resolve_relative_module(
    source_path: &str,
    specifier: &str,
    paths: &BTreeMap<String, usize>,
    extensions: &[&str],
) -> Vec<usize> {
    let parent = parent_path(source_path);
    let Some(base) = join_normalized(&parent, specifier) else {
        return Vec::new();
    };
    let mut candidates = vec![base.clone()];
    if Path::new(&base).extension().is_none() {
        candidates.extend(
            extensions
                .iter()
                .map(|extension| format!("{base}.{extension}")),
        );
        candidates.extend(
            extensions
                .iter()
                .map(|extension| format!("{base}/index.{extension}")),
        );
    }
    candidates
        .into_iter()
        .filter_map(|candidate| paths.get(&candidate).copied())
        .collect()
}

fn resolve_rust_module(
    source_path: &str,
    specifier: &str,
    paths: &BTreeMap<String, usize>,
) -> Vec<usize> {
    let mut segments = specifier.split("::").collect::<Vec<_>>();
    if segments.is_empty() {
        return Vec::new();
    }
    let source_parent = parent_path(source_path);
    let crate_root = rust_crate_root(source_path, paths);
    let base = match segments.first().copied() {
        Some("crate") => {
            segments.remove(0);
            crate_root
        }
        Some("self") => {
            segments.remove(0);
            source_parent
        }
        Some("super") => {
            while segments.first().copied() == Some("super") {
                segments.remove(0);
            }
            parent_path(&source_parent)
        }
        _ => source_parent,
    };
    if segments.is_empty() {
        return Vec::new();
    }
    // The final segment is commonly the imported declaration rather than a module.
    let module_segments = &segments[..segments.len().saturating_sub(1)];
    let module_path = if module_segments.is_empty() {
        base
    } else {
        join_normalized(&base, &module_segments.join("/")).unwrap_or(base)
    };
    let mut candidates = Vec::new();
    if module_segments.is_empty() {
        if let Some(index) = paths.get(source_path) {
            candidates.push(*index);
        }
    } else {
        for candidate in [format!("{module_path}.rs"), format!("{module_path}/mod.rs")] {
            if let Some(index) = paths.get(&candidate) {
                candidates.push(*index);
            }
        }
    }
    candidates
}

fn rust_crate_root(source_path: &str, paths: &BTreeMap<String, usize>) -> String {
    let mut directory = parent_path(source_path);
    let fallback = directory.clone();
    loop {
        let lib = join_normalized(&directory, "lib.rs").unwrap_or_else(|| "lib.rs".to_owned());
        let main = join_normalized(&directory, "main.rs").unwrap_or_else(|| "main.rs".to_owned());
        if paths.contains_key(&lib) || paths.contains_key(&main) {
            return directory;
        }
        if directory.is_empty() {
            return fallback;
        }
        directory = parent_path(&directory);
    }
}

fn starts_with_local_rust_root(specifier: &str) -> bool {
    matches!(
        specifier.split("::").next(),
        Some("crate" | "self" | "super")
    )
}

fn rust_inline_module_matches(file: &ResolutionFile, specifier: &str) -> bool {
    let module_name = specifier
        .split("::")
        .find(|segment| !matches!(*segment, "crate" | "self" | "super"));
    let Some(module_name) = module_name else {
        return false;
    };
    file.observations.iter().any(|observation| {
        observation.category == ObservationKind::Symbol
            && observation.kind == "module"
            && observation.spelling == module_name
    })
}

fn join_normalized(parent: &str, child: &str) -> Option<String> {
    let joined = if parent.is_empty() {
        PathBuf::from(child)
    } else {
        Path::new(parent).join(child)
    };
    let mut stack = Vec::new();
    for component in joined.components() {
        match component {
            Component::Normal(value) => stack.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                stack.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(stack.join("/"))
}

fn parent_path(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|parent| parent != ".")
        .unwrap_or_default()
}

fn declared_module(file: &ResolutionFile) -> Option<String> {
    if file.language == "csharp" {
        return file
            .observations
            .iter()
            .find(|item| item.category == ObservationKind::Symbol && item.kind == "namespace")
            .map(|item| item.spelling.clone());
    }
    file.observations
        .iter()
        .find(|item| item.category == ObservationKind::Import && item.kind == "package")
        .map(|item| item.spelling.clone())
}

fn is_module_declaration_observation(observation: &ResolutionObservation) -> bool {
    matches!(observation.kind.as_str(), "package" | "library")
}

fn is_local_export_observation(observation: &ResolutionObservation) -> bool {
    observation.kind == "export"
}

fn scope_chain(
    file: &ResolutionFile,
    scope_id: Option<&str>,
    cancellation: &CancellationContext,
) -> Result<Vec<String>, ResolutionError> {
    let scopes = file
        .observations
        .iter()
        .filter(|item| item.category == ObservationKind::Scope)
        .collect::<Vec<_>>();
    let Some(mut current) =
        scope_id.and_then(|id| scopes.iter().find(|scope| scope.id == id).copied())
    else {
        return Ok(scopes
            .iter()
            .filter(|scope| scope.kind == "file")
            .map(|scope| scope.id.clone())
            .collect());
    };
    let mut output = Vec::new();
    let mut visited = HashSet::new();
    loop {
        if cancellation.is_cancelled() {
            return Err(ResolutionError::Cancelled);
        }
        if output.len() >= 512 || !visited.insert(current.id.as_str()) {
            return Err(ResolutionError::InvalidScopeChain);
        }
        output.push(current.id.clone());
        if current.kind == "file" {
            break;
        }
        let parent = scopes
            .iter()
            .filter(|candidate| {
                candidate.id != current.id
                    && contains(candidate.syntax_range, current.syntax_range)
                    && (candidate.syntax_range != current.syntax_range || candidate.kind == "file")
            })
            .min_by_key(|candidate| {
                (
                    range_length(candidate.syntax_range),
                    candidate.kind == "file",
                    candidate.id.as_str(),
                )
            })
            .copied();
        let Some(parent) = parent else {
            break;
        };
        current = parent;
    }
    Ok(output)
}

fn contains(outer: ByteRange, inner: ByteRange) -> bool {
    outer.start() <= inner.start() && outer.end() >= inner.end()
}

fn range_length(range: ByteRange) -> u64 {
    range.end().saturating_sub(range.start())
}

fn occurrence_name(observation: &ResolutionObservation) -> String {
    split_qualified_name(&observation.spelling).1
}

fn split_qualified_name(value: &str) -> (Option<String>, String) {
    for separator in ["::", ".", "/"] {
        if let Some((qualifier, name)) = value.rsplit_once(separator) {
            return (Some(qualifier.to_owned()), name.to_owned());
        }
    }
    (None, value.to_owned())
}

fn imported_binding_name(language: &str, import: &ResolutionObservation) -> Option<String> {
    match language {
        "javascript" | "jsx" | "typescript" | "tsx" => {
            let imported = attribute_value(&import.attributes, "imported:")?;
            (imported != "*" && imported != "<side-effect>").then_some(imported)
        }
        "rust" => Some(
            import
                .spelling
                .rsplit("::")
                .next()
                .unwrap_or(import.spelling.as_str())
                .to_owned(),
        ),
        "java" => (!import
            .attributes
            .iter()
            .any(|item| item == "wildcard_import"))
        .then(|| {
            import
                .spelling
                .rsplit('.')
                .next()
                .unwrap_or(&import.spelling)
                .to_owned()
        }),
        _ => None,
    }
}

fn import_matches_occurrence(
    language: &str,
    import: &ResolutionObservation,
    qualifier: Option<&str>,
    name: &str,
) -> Option<String> {
    if language == "dart" && !dart_combinators_allow(import, name) {
        return None;
    }
    match language {
        "javascript" | "jsx" | "typescript" | "tsx" => {
            let imported = attribute_value(&import.attributes, "imported:")?;
            let local = import.alias.as_deref();
            if imported == "*" {
                (qualifier == local).then_some(name.to_owned())
            } else if qualifier.is_none() && local == Some(name) {
                Some(imported)
            } else {
                None
            }
        }
        "dart" => match import.alias.as_deref() {
            Some(prefix) if qualifier == Some(prefix) => Some(name.to_owned()),
            None if qualifier.is_none() => Some(name.to_owned()),
            _ => None,
        },
        "rust" => {
            let imported = import
                .spelling
                .rsplit("::")
                .next()
                .unwrap_or(&import.spelling);
            let local = import.alias.as_deref().unwrap_or(imported);
            (qualifier.is_none() && local == name).then_some(imported.to_owned())
        }
        "go" => (qualifier == import.alias.as_deref()).then_some(name.to_owned()),
        "java" => {
            if import
                .attributes
                .iter()
                .any(|item| item == "wildcard_import")
            {
                (qualifier.is_none()).then_some(name.to_owned())
            } else {
                let imported = import
                    .spelling
                    .rsplit('.')
                    .next()
                    .unwrap_or(&import.spelling);
                (qualifier.is_none() && imported == name).then_some(name.to_owned())
            }
        }
        "csharp" => match import.alias.as_deref() {
            Some(alias) if qualifier == Some(alias) => Some(name.to_owned()),
            None if qualifier.is_none() => Some(name.to_owned()),
            _ => None,
        },
        _ => None,
    }
}

fn dart_combinators_allow(import: &ResolutionObservation, name: &str) -> bool {
    import.attributes.iter().all(|attribute| {
        if let Some(names) = attribute.strip_prefix("show:") {
            names.split(',').any(|item| item == name)
        } else if let Some(names) = attribute.strip_prefix("hide:") {
            !names.split(',').any(|item| item == name)
        } else {
            true
        }
    })
}

fn attribute_value(attributes: &[String], prefix: &str) -> Option<String> {
    attributes
        .iter()
        .find_map(|attribute| attribute.strip_prefix(prefix).map(str::to_owned))
}

fn role_compatible(
    occurrence: &ResolutionObservation,
    declaration: &ResolutionObservation,
) -> bool {
    let occurrence_type = occurrence.attributes.iter().any(|item| item == "role:type");
    let occurrence_value = occurrence
        .attributes
        .iter()
        .any(|item| item == "role:value");
    let declaration_type = declaration
        .attributes
        .iter()
        .any(|item| item == "role:type")
        || is_type_kind(&declaration.kind);
    let declaration_value = declaration
        .attributes
        .iter()
        .any(|item| item == "role:value")
        || !matches!(
            declaration.kind.as_str(),
            "interface" | "type_alias" | "type_parameter"
        );
    (!occurrence_type || declaration_type) && (!occurrence_value || declaration_value)
}

fn is_module_visible(language: &str, symbol: &ResolutionObservation) -> bool {
    match language {
        "javascript" | "jsx" | "typescript" | "tsx" => {
            symbol.attributes.iter().any(|item| item == "exported")
        }
        "dart" => !symbol.spelling.starts_with('_'),
        "go" => symbol
            .spelling
            .chars()
            .next()
            .is_some_and(char::is_uppercase),
        "rust" => symbol
            .signature
            .as_deref()
            .is_some_and(|signature| signature.trim_start().starts_with("pub ")),
        _ => true,
    }
}

fn is_module_level_symbol(file: &ResolutionFile, symbol: &ResolutionObservation) -> bool {
    let Some(scope_id) = symbol.scope_id.as_deref() else {
        return true;
    };
    file.observations.iter().any(|scope| {
        scope.category == ObservationKind::Scope
            && scope.id == scope_id
            && matches!(scope.kind.as_str(), "file" | "namespace" | "module")
    })
}

fn is_dynamic(observation: &ResolutionObservation) -> bool {
    observation
        .attributes
        .iter()
        .any(|item| matches!(item.as_str(), "computed_target" | "dynamic_specifier"))
        || observation.limitations.iter().any(|item| {
            matches!(
                item.as_str(),
                "dynamic_property_target_not_resolved"
                    | "dynamic_module_specifier_not_resolved"
                    | "macro_not_expanded"
                    | "jsx_tag_not_a_call_edge"
            )
        })
}

fn has_uncertain_receiver(observation: &ResolutionObservation) -> bool {
    let (qualifier, _) = split_qualified_name(&observation.spelling);
    observation.receiver.is_some()
        || qualifier.is_some()
        || observation.limitations.iter().any(|item| {
            matches!(
                item.as_str(),
                "receiver_type_not_inferred"
                    | "receiver_type_or_import_prefix_not_inferred"
                    | "interface_virtual_or_extension_dispatch_not_resolved"
                    | "interface_or_virtual_dispatch_not_resolved"
                    | "dispatch_target_not_resolved"
            )
        })
}

fn is_callable_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function"
            | "function_binding"
            | "function_overload"
            | "method"
            | "trait_method"
            | "interface_method"
            | "local_function"
            | "constructor"
            | "factory_constructor"
            | "redirecting_factory_constructor"
            | "getter"
            | "setter"
            | "macro"
            | "variable"
            | "local"
            | "parameter"
    )
}

fn is_callable_container(kind: &str) -> bool {
    is_callable_kind(kind)
        || matches!(
            kind,
            "class" | "struct" | "enum" | "trait" | "interface" | "record" | "mixin" | "extension"
        )
}

fn is_container_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class"
            | "struct"
            | "enum"
            | "trait"
            | "interface"
            | "record"
            | "mixin"
            | "extension"
            | "extension_type"
            | "namespace"
            | "module"
    ) || is_callable_kind(kind)
}

fn is_member_kind(kind: &str) -> bool {
    matches!(
        kind,
        "method"
            | "trait_method"
            | "interface_method"
            | "field"
            | "property"
            | "getter"
            | "setter"
            | "constructor"
            | "factory_constructor"
    )
}

fn is_type_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class"
            | "struct"
            | "enum"
            | "trait"
            | "interface"
            | "record"
            | "mixin"
            | "extension"
            | "extension_type"
            | "type"
            | "type_alias"
            | "type_parameter"
            | "namespace"
            | "module"
    )
}

fn is_order_sensitive(kind: &str) -> bool {
    matches!(kind, "local" | "pattern_variable")
}

fn edge_id(
    relationship: RelationshipKind,
    source_path: &str,
    source_id: &str,
    target: Option<&EdgeTarget>,
    resolution: ResolutionLabel,
    rule: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [
        relationship.as_str(),
        source_path,
        source_id,
        resolution.as_str(),
        rule,
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    match target {
        Some(EdgeTarget::LocalFile { relative_path }) => {
            hasher.update(b"local-file\0");
            hasher.update(relative_path.as_bytes());
        }
        Some(EdgeTarget::LocalSymbol(symbol)) => {
            hasher.update(b"local-symbol\0");
            hasher.update(symbol.relative_path.as_bytes());
            hasher.update(&[0]);
            hasher.update(symbol.symbol_id.as_bytes());
        }
        Some(EdgeTarget::External { node_id }) => {
            hasher.update(b"external\0");
            hasher.update(node_id.as_bytes());
        }
        None => {
            hasher.update(b"unresolved");
        }
    };
    hasher.finalize().to_hex().to_string()
}

fn external_node_id(language: &str, kind: &str, label: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [language, kind, label] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    format!("external:{}", hasher.finalize().to_hex())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphLimits {
    pub depth: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
}

impl Default for GraphLimits {
    fn default() -> Self {
        Self {
            depth: 2,
            max_nodes: 500,
            max_edges: 1_000,
        }
    }
}

impl GraphLimits {
    fn validate(self) -> Result<(), GraphError> {
        if self.depth == 0
            || self.depth > 8
            || self.max_nodes == 0
            || self.max_nodes > 10_000
            || self.max_edges == 0
            || self.max_edges > 100_000
        {
            return Err(GraphError::InvalidLimits);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEdge {
    pub id: String,
    pub relationship: RelationshipKind,
    pub source_symbol_id: Option<String>,
    pub source_observation_id: String,
    pub target_symbol_id: Option<String>,
    pub resolution: ResolutionLabel,
}

pub trait GraphStore {
    type Error: Error + Send + Sync + 'static;

    fn adjacent_edges(
        &self,
        generation_id: &GenerationId,
        symbol_id: &str,
        direction: GraphDirection,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<GraphEdge>, Self::Error>;

    fn call_site_edges(
        &self,
        generation_id: &GenerationId,
        symbol_id: &str,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<GraphEdge>, Self::Error>;
}

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("invalid graph limits")]
    InvalidLimits,
    #[error("graph store operation failed: {0}")]
    Store(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphTraversal {
    pub edges: Vec<GraphEdge>,
    pub visited_symbols: Vec<String>,
    pub truncated: bool,
}

pub struct GraphService<'a, S> {
    store: &'a S,
}

impl<'a, S> GraphService<'a, S>
where
    S: GraphStore,
{
    pub const fn new(store: &'a S) -> Self {
        Self { store }
    }

    pub fn neighbors(
        &self,
        generation_id: &GenerationId,
        start_symbol_id: &str,
        direction: GraphDirection,
        include_candidates: bool,
        limits: GraphLimits,
    ) -> Result<GraphTraversal, GraphError> {
        limits.validate()?;
        if start_symbol_id.is_empty() || start_symbol_id.len() > 256 {
            return Err(GraphError::InvalidLimits);
        }
        let mut queue = VecDeque::from([(start_symbol_id.to_owned(), 0_usize)]);
        let mut visited = BTreeSet::from([start_symbol_id.to_owned()]);
        let mut edge_ids = BTreeSet::new();
        let mut edges = Vec::new();
        let mut truncated = false;
        while let Some((symbol_id, depth)) = queue.pop_front() {
            if depth >= limits.depth {
                continue;
            }
            let remaining = limits.max_edges.saturating_sub(edges.len());
            if remaining == 0 {
                truncated = true;
                break;
            }
            let adjacent = self
                .store
                .adjacent_edges(
                    generation_id,
                    &symbol_id,
                    direction,
                    include_candidates,
                    remaining.saturating_add(1),
                )
                .map_err(|error| GraphError::Store(error.to_string()))?;
            if adjacent.len() > remaining {
                truncated = true;
            }
            for edge in adjacent.into_iter().take(remaining) {
                let next = match direction {
                    GraphDirection::Incoming => edge.source_symbol_id.clone(),
                    GraphDirection::Outgoing => edge.target_symbol_id.clone(),
                };
                if edge_ids.insert(edge.id.clone()) {
                    edges.push(edge);
                }
                if let Some(next) = next.as_ref()
                    && visited.len() < limits.max_nodes
                    && visited.insert(next.clone())
                {
                    queue.push_back((next.clone(), depth.saturating_add(1)));
                } else if next.is_some() && visited.len() >= limits.max_nodes {
                    truncated = true;
                }
            }
        }
        Ok(GraphTraversal {
            edges,
            visited_symbols: visited.into_iter().collect(),
            truncated,
        })
    }

    pub fn call_sites(
        &self,
        generation_id: &GenerationId,
        symbol_id: &str,
        include_candidates: bool,
        limit: usize,
    ) -> Result<Vec<GraphEdge>, GraphError> {
        if symbol_id.is_empty() || symbol_id.len() > 256 || !(1..=1_000).contains(&limit) {
            return Err(GraphError::InvalidLimits);
        }
        self.store
            .call_site_edges(generation_id, symbol_id, include_candidates, limit)
            .map_err(|error| GraphError::Store(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    fn range(start: u64, end: u64) -> ByteRange {
        ByteRange::new(start, end).expect("valid test range")
    }

    fn observation(
        category: ObservationKind,
        id: &str,
        kind: &str,
        spelling: &str,
        bytes: (u64, u64),
        syntax: (u64, u64),
        scope_id: Option<&str>,
    ) -> ResolutionObservation {
        ResolutionObservation {
            category,
            id: id.to_owned(),
            kind: kind.to_owned(),
            spelling: spelling.to_owned(),
            byte_range: range(bytes.0, bytes.1),
            syntax_range: range(syntax.0, syntax.1),
            scope_id: scope_id.map(str::to_owned),
            container: None,
            signature: None,
            receiver: None,
            alias: None,
            attributes: Vec::new(),
            limitations: Vec::new(),
        }
    }

    #[test]
    fn equal_range_file_scope_terminates_and_observes_cancellation() {
        let file = ResolutionFile {
            relative_path: "a.rs".into(),
            language: "rust".into(),
            observations: vec![
                observation(
                    ObservationKind::Scope,
                    "file",
                    "file",
                    "<file>",
                    (0, 20),
                    (0, 20),
                    Some("function"),
                ),
                observation(
                    ObservationKind::Scope,
                    "function",
                    "function",
                    "main",
                    (0, 20),
                    (0, 20),
                    Some("file"),
                ),
                observation(
                    ObservationKind::Scope,
                    "body",
                    "block",
                    "",
                    (9, 20),
                    (9, 20),
                    Some("function"),
                ),
            ],
        };
        let cancel = CancellationContext::default();
        assert_eq!(
            scope_chain(&file, Some("body"), &cancel).expect("acyclic chain"),
            ["body", "function", "file"]
        );
        cancel.cancel();
        assert!(matches!(
            scope_chain(&file, Some("body"), &cancel),
            Err(ResolutionError::Cancelled)
        ));
    }

    #[test]
    fn dart_combinators_intersect_shows_and_subtract_hides() {
        let mut import = observation(
            ObservationKind::Import,
            "import",
            "import",
            "a.dart",
            (0, 1),
            (0, 1),
            None,
        );
        import.attributes = vec!["show:a,b".into(), "show:b,c".into(), "hide:c".into()];
        assert!(dart_combinators_allow(&import, "b"));
        for name in ["a", "c", "d"] {
            assert!(!dart_combinators_allow(&import, name));
        }
    }

    #[test]
    fn lexical_shadowing_overloads_and_dynamic_targets_preserve_certainty() {
        let mut file_scope = observation(
            ObservationKind::Scope,
            "scope-file",
            "file",
            "<file>",
            (0, 200),
            (0, 200),
            None,
        );
        file_scope.scope_id = None;
        let function_scope = observation(
            ObservationKind::Scope,
            "scope-function",
            "function",
            "use_it",
            (50, 190),
            (50, 190),
            Some("scope-file"),
        );
        let top = observation(
            ObservationKind::Symbol,
            "top-value",
            "variable",
            "value",
            (4, 9),
            (0, 10),
            Some("scope-file"),
        );
        let local = observation(
            ObservationKind::Symbol,
            "local-value",
            "local",
            "value",
            (80, 85),
            (76, 90),
            Some("scope-function"),
        );
        let mut reference = observation(
            ObservationKind::Reference,
            "ref-value",
            "identifier",
            "value",
            (100, 105),
            (100, 105),
            Some("scope-function"),
        );
        reference.attributes.push("role:value".to_owned());
        let overload_a = observation(
            ObservationKind::Symbol,
            "overload-a",
            "function",
            "run",
            (12, 15),
            (10, 30),
            Some("scope-file"),
        );
        let overload_b = observation(
            ObservationKind::Symbol,
            "overload-b",
            "function",
            "run",
            (32, 35),
            (30, 48),
            Some("scope-file"),
        );
        let call = observation(
            ObservationKind::CallSite,
            "call-run",
            "function",
            "run",
            (120, 123),
            (120, 126),
            Some("scope-function"),
        );
        let mut dynamic = observation(
            ObservationKind::CallSite,
            "call-dynamic",
            "method",
            "obj[key]",
            (130, 138),
            (130, 141),
            Some("scope-function"),
        );
        dynamic.attributes.push("computed_target".to_owned());
        let input = ResolutionInput {
            generation_id: GenerationId::new("g-test").expect("valid generation"),
            files: vec![ResolutionFile {
                relative_path: "sample.ts".to_owned(),
                language: "typescript".to_owned(),
                observations: vec![
                    file_scope,
                    function_scope,
                    top,
                    local,
                    reference,
                    overload_a,
                    overload_b,
                    call,
                    dynamic,
                ],
            }],
            manifests: Vec::new(),
        };
        let graph = resolve_generation(
            &input,
            ResolutionLimits::default(),
            &CancellationContext::default(),
        )
        .expect("resolve graph");
        let reference_edge = graph
            .edges
            .iter()
            .find(|edge| edge.source_observation_id == "ref-value")
            .expect("reference edge");
        assert_eq!(
            reference_edge.resolution,
            ResolutionLabel::LexicallyResolved
        );
        assert!(matches!(
            &reference_edge.target,
            Some(EdgeTarget::LocalSymbol(target)) if target.symbol_id == "local-value"
        ));
        let overloads = graph
            .edges
            .iter()
            .filter(|edge| edge.source_observation_id == "call-run")
            .collect::<Vec<_>>();
        assert_eq!(overloads.len(), 2);
        assert!(overloads.iter().all(|edge| {
            edge.resolution == ResolutionLabel::Candidate && edge.candidate_count == 2
        }));
        let dynamic_edge = graph
            .edges
            .iter()
            .find(|edge| edge.source_observation_id == "call-dynamic")
            .expect("dynamic edge");
        assert_eq!(dynamic_edge.resolution, ResolutionLabel::Unresolved);
        assert!(dynamic_edge.target.is_none());
    }

    #[test]
    fn relative_alias_reexports_are_bounded_and_cycle_safe() {
        let scope = |id: &str| {
            observation(
                ObservationKind::Scope,
                id,
                "file",
                "<file>",
                (0, 100),
                (0, 100),
                None,
            )
        };
        let mut exported = observation(
            ObservationKind::Symbol,
            "symbol-run",
            "function",
            "run",
            (10, 13),
            (0, 20),
            Some("scope-provider"),
        );
        exported.attributes.push("exported".to_owned());
        let mut reexport = observation(
            ObservationKind::Import,
            "reexport-run",
            "reexport",
            "./provider",
            (10, 20),
            (0, 22),
            Some("scope-middle"),
        );
        reexport.alias = Some("execute".to_owned());
        reexport.attributes.push("local:run".to_owned());
        let mut import = observation(
            ObservationKind::Import,
            "import-execute",
            "esm_import",
            "./middle",
            (0, 20),
            (0, 25),
            Some("scope-consumer"),
        );
        import.alias = Some("go".to_owned());
        import.attributes.push("imported:execute".to_owned());
        let call = observation(
            ObservationKind::CallSite,
            "call-go",
            "function",
            "go",
            (40, 42),
            (40, 44),
            Some("scope-consumer"),
        );
        let input = ResolutionInput {
            generation_id: GenerationId::new("g-modules").expect("valid generation"),
            files: vec![
                ResolutionFile {
                    relative_path: "provider.ts".to_owned(),
                    language: "typescript".to_owned(),
                    observations: vec![scope("scope-provider"), exported],
                },
                ResolutionFile {
                    relative_path: "middle.ts".to_owned(),
                    language: "typescript".to_owned(),
                    observations: vec![scope("scope-middle"), reexport],
                },
                ResolutionFile {
                    relative_path: "consumer.ts".to_owned(),
                    language: "typescript".to_owned(),
                    observations: vec![scope("scope-consumer"), import, call],
                },
            ],
            manifests: Vec::new(),
        };
        let graph = resolve_generation(
            &input,
            ResolutionLimits::default(),
            &CancellationContext::default(),
        )
        .expect("resolve re-export");
        let edge = graph
            .edges
            .iter()
            .find(|edge| edge.source_observation_id == "call-go")
            .expect("aliased call edge");
        assert_eq!(edge.resolution, ResolutionLabel::LexicallyResolved);
        assert!(matches!(
            &edge.target,
            Some(EdgeTarget::LocalSymbol(target)) if target.symbol_id == "symbol-run"
        ));
    }

    #[test]
    fn supported_go_and_dart_manifests_are_parsed_only_as_module_data() {
        let scope = |id: &str| {
            observation(
                ObservationKind::Scope,
                id,
                "file",
                "<file>",
                (0, 100),
                (0, 100),
                None,
            )
        };
        let mut go_package = observation(
            ObservationKind::Import,
            "go-package",
            "package",
            "pkg",
            (0, 3),
            (0, 12),
            Some("go-provider-scope"),
        );
        go_package.alias = None;
        let mut go_symbol = observation(
            ObservationKind::Symbol,
            "go-run",
            "function",
            "Run",
            (20, 23),
            (15, 40),
            Some("go-provider-scope"),
        );
        go_symbol.signature = Some("func Run()".to_owned());
        let mut go_import = observation(
            ObservationKind::Import,
            "go-import",
            "import",
            "example.test/app/pkg",
            (10, 30),
            (0, 32),
            Some("go-consumer-scope"),
        );
        go_import.alias = Some("pkg".to_owned());

        let dart_symbol = observation(
            ObservationKind::Symbol,
            "dart-twice",
            "function",
            "twice",
            (4, 9),
            (0, 30),
            Some("dart-provider-scope"),
        );
        let dart_import = observation(
            ObservationKind::Import,
            "dart-import",
            "import",
            "package:sample/math.dart",
            (7, 31),
            (0, 33),
            Some("dart-consumer-scope"),
        );
        let input = ResolutionInput {
            generation_id: GenerationId::new("g-manifests").expect("generation"),
            files: vec![
                ResolutionFile {
                    relative_path: "pkg/math.go".to_owned(),
                    language: "go".to_owned(),
                    observations: vec![scope("go-provider-scope"), go_package, go_symbol],
                },
                ResolutionFile {
                    relative_path: "cmd/main.go".to_owned(),
                    language: "go".to_owned(),
                    observations: vec![scope("go-consumer-scope"), go_import],
                },
                ResolutionFile {
                    relative_path: "lib/math.dart".to_owned(),
                    language: "dart".to_owned(),
                    observations: vec![scope("dart-provider-scope"), dart_symbol],
                },
                ResolutionFile {
                    relative_path: "bin/main.dart".to_owned(),
                    language: "dart".to_owned(),
                    observations: vec![scope("dart-consumer-scope"), dart_import],
                },
            ],
            manifests: vec![
                ManifestData {
                    relative_path: "go.mod".to_owned(),
                    contents: "module example.test/app\n\ngo 1.24\n".to_owned(),
                },
                ManifestData {
                    relative_path: "pubspec.yaml".to_owned(),
                    contents: "name: sample\nenvironment:\n  sdk: ^3.6.0\n".to_owned(),
                },
            ],
        };
        let graph = resolve_generation(
            &input,
            ResolutionLimits::default(),
            &CancellationContext::default(),
        )
        .expect("resolve manifest modules");
        assert!(graph.edges.iter().any(|edge| {
            edge.source_observation_id == "go-import"
                && edge.resolution == ResolutionLabel::LexicallyResolved
                && matches!(
                    &edge.target,
                    Some(EdgeTarget::LocalFile { relative_path }) if relative_path == "pkg/math.go"
                )
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.source_observation_id == "dart-import"
                && edge.resolution == ResolutionLabel::LexicallyResolved
                && matches!(
                    &edge.target,
                    Some(EdgeTarget::LocalFile { relative_path }) if relative_path == "lib/math.dart"
                )
        }));
    }

    #[test]
    fn containment_points_from_container_to_child_and_rust_crate_paths_use_the_root() {
        let file_scope = observation(
            ObservationKind::Scope,
            "scope-file",
            "file",
            "<file>",
            (0, 100),
            (0, 100),
            None,
        );
        let container = observation(
            ObservationKind::Symbol,
            "container",
            "class",
            "Outer",
            (6, 11),
            (0, 90),
            Some("scope-file"),
        );
        let child = observation(
            ObservationKind::Symbol,
            "child",
            "method",
            "run",
            (30, 33),
            (20, 60),
            Some("scope-file"),
        );
        let input = ResolutionInput {
            generation_id: GenerationId::new("g-containment").expect("generation"),
            files: vec![ResolutionFile {
                relative_path: "sample.cs".to_owned(),
                language: "csharp".to_owned(),
                observations: vec![file_scope, container, child],
            }],
            manifests: Vec::new(),
        };
        let graph = resolve_generation(
            &input,
            ResolutionLimits::default(),
            &CancellationContext::default(),
        )
        .expect("resolve containment");
        assert!(graph.edges.iter().any(|edge| {
            edge.relationship == RelationshipKind::Contains
                && edge
                    .source_symbol
                    .as_ref()
                    .map(|symbol| symbol.symbol_id.as_str())
                    == Some("container")
                && matches!(
                    &edge.target,
                    Some(EdgeTarget::LocalSymbol(symbol)) if symbol.symbol_id == "child"
                )
        }));

        let paths = BTreeMap::from([
            ("crates/app/src/lib.rs".to_owned(), 0),
            ("crates/app/src/domain/service.rs".to_owned(), 1),
            ("crates/app/src/shared/model.rs".to_owned(), 2),
        ]);
        assert_eq!(
            resolve_rust_module(
                "crates/app/src/domain/service.rs",
                "crate::shared::model::Record",
                &paths,
            ),
            vec![2]
        );
    }

    #[derive(Default)]
    struct MemoryGraphStore {
        edges: Vec<GraphEdge>,
    }

    impl GraphStore for MemoryGraphStore {
        type Error = Infallible;

        fn adjacent_edges(
            &self,
            _generation_id: &GenerationId,
            symbol_id: &str,
            direction: GraphDirection,
            include_candidates: bool,
            limit: usize,
        ) -> Result<Vec<GraphEdge>, Self::Error> {
            Ok(self
                .edges
                .iter()
                .filter(|edge| include_candidates || edge.resolution != ResolutionLabel::Candidate)
                .filter(|edge| match direction {
                    GraphDirection::Incoming => edge.target_symbol_id.as_deref() == Some(symbol_id),
                    GraphDirection::Outgoing => edge.source_symbol_id.as_deref() == Some(symbol_id),
                })
                .take(limit)
                .cloned()
                .collect())
        }

        fn call_site_edges(
            &self,
            _generation_id: &GenerationId,
            symbol_id: &str,
            include_candidates: bool,
            limit: usize,
        ) -> Result<Vec<GraphEdge>, Self::Error> {
            Ok(self
                .edges
                .iter()
                .filter(|edge| edge.relationship == RelationshipKind::Calls)
                .filter(|edge| edge.target_symbol_id.as_deref() == Some(symbol_id))
                .filter(|edge| include_candidates || edge.resolution != ResolutionLabel::Candidate)
                .take(limit)
                .cloned()
                .collect())
        }
    }

    #[test]
    fn graph_reads_exclude_candidates_and_stop_on_cycles_and_limits() {
        let edge = |id: &str, source: &str, target: &str, resolution| GraphEdge {
            id: id.to_owned(),
            relationship: RelationshipKind::Calls,
            source_symbol_id: Some(source.to_owned()),
            source_observation_id: format!("site-{id}"),
            target_symbol_id: Some(target.to_owned()),
            resolution,
        };
        let store = MemoryGraphStore {
            edges: vec![
                edge("ab", "a", "b", ResolutionLabel::LexicallyResolved),
                edge("ba", "b", "a", ResolutionLabel::LexicallyResolved),
                edge("ac", "a", "c", ResolutionLabel::Candidate),
            ],
        };
        let service = GraphService::new(&store);
        let generation = GenerationId::new("g-graph").expect("valid generation");
        let default = service
            .neighbors(
                &generation,
                "a",
                GraphDirection::Outgoing,
                false,
                GraphLimits::default(),
            )
            .expect("bounded graph");
        assert_eq!(default.edges.len(), 2);
        assert!(!default.edges.iter().any(|edge| edge.id == "ac"));
        let candidates = service
            .neighbors(
                &generation,
                "a",
                GraphDirection::Outgoing,
                true,
                GraphLimits {
                    depth: 2,
                    max_nodes: 2,
                    max_edges: 2,
                },
            )
            .expect("candidate graph");
        assert!(candidates.truncated);
        assert!(candidates.edges.len() <= 2);
        assert!(candidates.visited_symbols.len() <= 2);
    }
}
