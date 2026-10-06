use std::{collections::HashMap, ops::ControlFlow, path::Path};

use ca_core::{ByteRange, CancellationContext};
use tree_sitter::{
    Node, ParseOptions, Parser, Point, Query, QueryCursor, QueryCursorOptions, StreamingIterator,
};

use crate::{
    Diagnostic, DiagnosticSeverity, ExtractionResult, LanguageError, LanguageId, LanguageProvider,
    LanguageRegistry, ParseCoverage, ParseLimits, ParseStage, ProgressEvent, SourcePoint,
    SourceRange,
    adapters::{RawCapture, extract},
    registry::{compile_query_with_context, validate_capture_names},
};

struct WorkerLanguage {
    provider: LanguageProvider,
    parser: Parser,
    queries: Vec<Query>,
}

#[derive(Default)]
pub struct ParserWorker {
    languages: HashMap<LanguageId, WorkerLanguage>,
}

impl ParserWorker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse(
        &mut self,
        language: LanguageId,
        source: &[u8],
        limits: ParseLimits,
        cancellation: &CancellationContext,
    ) -> Result<ExtractionResult, LanguageError> {
        self.parse_file_with_progress(
            language,
            Path::new("<memory>"),
            source,
            limits,
            cancellation,
            &mut |_| ControlFlow::Continue(()),
        )
    }

    pub fn parse_file(
        &mut self,
        language: LanguageId,
        relative_path: &Path,
        source: &[u8],
        limits: ParseLimits,
        cancellation: &CancellationContext,
    ) -> Result<ExtractionResult, LanguageError> {
        self.parse_file_with_progress(
            language,
            relative_path,
            source,
            limits,
            cancellation,
            &mut |_| ControlFlow::Continue(()),
        )
    }

    pub fn parse_with_progress<F>(
        &mut self,
        language: LanguageId,
        source: &[u8],
        limits: ParseLimits,
        cancellation: &CancellationContext,
        progress: &mut F,
    ) -> Result<ExtractionResult, LanguageError>
    where
        F: FnMut(ProgressEvent) -> ControlFlow<()>,
    {
        self.parse_file_with_progress(
            language,
            Path::new("<memory>"),
            source,
            limits,
            cancellation,
            progress,
        )
    }

    pub fn parse_file_with_progress<F>(
        &mut self,
        language: LanguageId,
        relative_path: &Path,
        source: &[u8],
        limits: ParseLimits,
        cancellation: &CancellationContext,
        progress: &mut F,
    ) -> Result<ExtractionResult, LanguageError>
    where
        F: FnMut(ProgressEvent) -> ControlFlow<()>,
    {
        limits.validate()?;
        if source.len() > limits.max_source_bytes {
            return Err(LanguageError::SourceTooLarge {
                actual: source.len(),
                limit: limits.max_source_bytes,
            });
        }
        std::str::from_utf8(source).map_err(|error| LanguageError::InvalidUtf8 {
            valid_up_to: error.valid_up_to(),
        })?;
        if cancellation.is_cancelled() {
            return Err(LanguageError::Cancelled {
                stage: ParseStage::Parse,
            });
        }

        self.initialize(language)?;
        let state = self
            .languages
            .get_mut(&language)
            .ok_or(LanguageError::WorkerStateMissing { language })?;

        let mut parse_callbacks = 0_usize;
        let mut parse_abort = None;
        let mut progress_callback = |state: &tree_sitter::ParseState| {
            parse_callbacks += 1;
            let event = ProgressEvent {
                stage: ParseStage::Parse,
                current_byte_offset: state.current_byte_offset(),
                callback_count: parse_callbacks,
            };
            if cancellation.is_cancelled() {
                parse_abort = Some(LanguageError::Cancelled {
                    stage: ParseStage::Parse,
                });
                return ControlFlow::Break(());
            }
            if parse_callbacks > limits.max_progress_callbacks {
                parse_abort = Some(LanguageError::WorkBudgetExceeded {
                    stage: ParseStage::Parse,
                });
                return ControlFlow::Break(());
            }
            if progress(event).is_break() {
                parse_abort = Some(LanguageError::ProgressStopped {
                    stage: ParseStage::Parse,
                });
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        };
        let mut read = |offset: usize, _position: Point| source.get(offset..).unwrap_or_default();
        let tree = state.parser.parse_with_options(
            &mut read,
            None,
            Some(ParseOptions::new().progress_callback(&mut progress_callback)),
        );
        if let Some(error) = parse_abort {
            state.parser.reset();
            return Err(error);
        }
        let tree = tree.ok_or_else(|| {
            state.parser.reset();
            LanguageError::ParseFailed { language }
        })?;
        if cancellation.is_cancelled() {
            state.parser.reset();
            return Err(LanguageError::Cancelled {
                stage: ParseStage::Query,
            });
        }

        let root = tree.root_node();
        let (mut diagnostics, nodes_visited, traversal_truncated) =
            syntax_diagnostics(root, limits)?;
        let mut captures = Vec::new();
        let mut captures_truncated = false;
        let mut query_match_limit_exceeded = false;
        let mut query_callbacks = 0_usize;

        for (query_index, query) in state.queries.iter().enumerate() {
            let asset = state
                .provider
                .queries()
                .get(query_index)
                .copied()
                .ok_or(LanguageError::WorkerStateMissing { language })?;
            let mut cursor = QueryCursor::new();
            cursor.set_match_limit(limits.query_match_limit);
            let mut query_abort = None;
            let mut query_progress = |cursor_state: &tree_sitter::QueryCursorState| {
                query_callbacks += 1;
                let event = ProgressEvent {
                    stage: ParseStage::Query,
                    current_byte_offset: cursor_state.current_byte_offset(),
                    callback_count: query_callbacks,
                };
                if cancellation.is_cancelled() {
                    query_abort = Some(LanguageError::Cancelled {
                        stage: ParseStage::Query,
                    });
                    return ControlFlow::Break(());
                }
                if query_callbacks > limits.max_progress_callbacks {
                    query_abort = Some(LanguageError::WorkBudgetExceeded {
                        stage: ParseStage::Query,
                    });
                    return ControlFlow::Break(());
                }
                if progress(event).is_break() {
                    query_abort = Some(LanguageError::ProgressStopped {
                        stage: ParseStage::Query,
                    });
                    return ControlFlow::Break(());
                }
                ControlFlow::Continue(())
            };
            {
                let options = QueryCursorOptions::new().progress_callback(&mut query_progress);
                let mut matches = cursor.matches_with_options(query, root, source, options);
                'matches: while let Some(query_match) = matches.next() {
                    for capture in query_match.captures() {
                        if captures.len() >= limits.max_captures {
                            captures_truncated = true;
                            break 'matches;
                        }
                        let capture_name = query
                            .capture_names()
                            .get(capture.index as usize)
                            .copied()
                            .ok_or_else(|| LanguageError::InvalidCaptureContract {
                                language,
                                query_name: asset.name.to_owned(),
                                capture: format!("unknown capture index {}", capture.index),
                            })?;
                        captures.push(RawCapture {
                            name: capture_name.to_owned(),
                            node: capture.node,
                        });
                    }
                }
            }
            if let Some(error) = query_abort {
                state.parser.reset();
                return Err(error);
            }
            query_match_limit_exceeded |= cursor.did_exceed_match_limit();
            if captures_truncated {
                break;
            }
        }

        if captures_truncated {
            diagnostics.push(Diagnostic {
                code: "capture_limit_reached".to_owned(),
                message: format!(
                    "query capture output was truncated at {} observations",
                    limits.max_captures
                ),
                severity: DiagnosticSeverity::Warning,
                range: None,
            });
        }
        if query_match_limit_exceeded {
            diagnostics.push(Diagnostic {
                code: "query_match_limit_exceeded".to_owned(),
                message: format!(
                    "Tree-sitter query in-progress match limit {} was exceeded",
                    limits.query_match_limit
                ),
                severity: DiagnosticSeverity::Warning,
                range: None,
            });
        }

        let mut extracted = extract(language, relative_path, source, &captures)?;
        diagnostics.append(&mut extracted.diagnostics);
        let query_fingerprint = state.provider.query_fingerprint();

        Ok(ExtractionResult {
            language,
            source_hash: blake3::hash(source).to_hex().to_string(),
            grammar_fingerprint: state.provider.grammar_fingerprint(),
            query_fingerprint,
            extractor_fingerprint: state.provider.extractor_fingerprint(),
            capabilities: state.provider.capabilities,
            declarations: extracted.declarations,
            scopes: extracted.scopes,
            imports: extracted.imports,
            references: extracted.references,
            call_sites: extracted.call_sites,
            conditions: extracted.conditions,
            diagnostics,
            coverage: ParseCoverage {
                root_has_error: root.has_error(),
                syntax_nodes_visited: nodes_visited,
                syntax_traversal_truncated: traversal_truncated,
                captures_truncated,
                query_match_limit_exceeded,
            },
        })
    }

    fn initialize(&mut self, language: LanguageId) -> Result<(), LanguageError> {
        if self.languages.contains_key(&language) {
            return Ok(());
        }
        let provider = LanguageRegistry::provider(language);
        let grammar = provider.language();
        let mut parser = Parser::new();
        parser
            .set_language(&grammar)
            .map_err(|error| LanguageError::GrammarLoad {
                language,
                message: error.to_string(),
            })?;
        let mut queries = Vec::with_capacity(provider.queries().len());
        for asset in provider.queries() {
            let query = compile_query_with_context(language, &grammar, asset.name, asset.source)?;
            validate_capture_names(language, asset.name, &query)?;
            queries.push(query);
        }
        self.languages.insert(
            language,
            WorkerLanguage {
                provider,
                parser,
                queries,
            },
        );
        Ok(())
    }
}

fn syntax_diagnostics(
    root: Node<'_>,
    limits: ParseLimits,
) -> Result<(Vec<Diagnostic>, usize, bool), LanguageError> {
    let mut diagnostics = Vec::new();
    let mut cursor = root.walk();
    let mut visited = 0_usize;
    loop {
        let node = cursor.node();
        visited += 1;
        if (node.is_error() || node.is_missing()) && diagnostics.len() < limits.max_diagnostics {
            diagnostics.push(Diagnostic {
                code: if node.is_missing() {
                    "syntax_missing".to_owned()
                } else {
                    "syntax_error".to_owned()
                },
                message: if node.is_missing() {
                    format!("Tree-sitter reported missing syntax: {}", node.kind())
                } else {
                    "Tree-sitter reported an ERROR node".to_owned()
                },
                severity: DiagnosticSeverity::Warning,
                range: Some(node_range(node)?),
            });
        }
        if visited >= limits.max_syntax_nodes {
            diagnostics.push(Diagnostic {
                code: "syntax_traversal_limit_reached".to_owned(),
                message: format!(
                    "syntax diagnostic traversal stopped after {} nodes",
                    limits.max_syntax_nodes
                ),
                severity: DiagnosticSeverity::Warning,
                range: None,
            });
            return Ok((diagnostics, visited, true));
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok((diagnostics, visited, false));
            }
        }
    }
}

pub(crate) fn node_range(node: Node<'_>) -> Result<SourceRange, LanguageError> {
    let start_byte =
        u64::try_from(node.start_byte()).map_err(|_| LanguageError::CoordinateOverflow)?;
    let end_byte = u64::try_from(node.end_byte()).map_err(|_| LanguageError::CoordinateOverflow)?;
    Ok(SourceRange {
        bytes: ByteRange::new(start_byte, end_byte)
            .map_err(|_| LanguageError::CoordinateOverflow)?,
        start: point(node.start_position())?,
        end: point(node.end_position())?,
    })
}

fn point(point: Point) -> Result<SourcePoint, LanguageError> {
    Ok(SourcePoint {
        line: u32::try_from(point.row + 1).map_err(|_| LanguageError::CoordinateOverflow)?,
        byte_column: u32::try_from(point.column).map_err(|_| LanguageError::CoordinateOverflow)?,
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, ops::ControlFlow, path::Path};

    use super::*;

    fn parse(language: LanguageId, source: &[u8]) -> ExtractionResult {
        ParserWorker::new()
            .parse(
                language,
                source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("test source should parse")
    }

    #[test]
    fn member_selectors_preserve_receivers_without_decorating_bare_names() {
        let cases: &[(LanguageId, &[u8])] = &[
            (LanguageId::Rust, b"struct S { value:i32 } fn read(obj:S){let value=9;let x=obj.value;let y=value;}"),
            (LanguageId::Go, b"package p\ntype S struct{ Value int };func read(obj S){Value:=9; x:=obj.Value; _=x;_=Value}"),
            (LanguageId::CSharp, b"class S {int value;void Read(S obj){int value=9;int x=obj.value;int y=value;}}"),
            (LanguageId::Java, b"class S {int value;void read(S obj){int value=9;int x=obj.value;int y=value;}}"),
            (LanguageId::Dart, b"class S {int value=1;void read(S obj){var value=9;var x=obj.value;var y=value;}}"),
        ];
        for &(language, source) in cases {
            let result = parse(language, source);
            assert!(!result.coverage.root_has_error, "{language:?}");
            let selector = result
                .references
                .iter()
                .find(|item| item.receiver_type.as_deref() == Some("obj"))
                .expect("selected property receiver");
            assert!(
                selector
                    .limitations
                    .iter()
                    .any(|item| item == "receiver_type_not_inferred")
            );
            assert!(
                result
                    .references
                    .iter()
                    .any(|item| item.spelling == selector.spelling
                        && item.range.bytes.start() > selector.range.bytes.start()
                        && item.receiver_type.is_none()),
                "bare name stays lexical: {language:?}"
            );
        }
    }

    #[test]
    fn arrow_and_function_expression_exports_are_visible_in_all_ecma_dialects() {
        for language in [
            LanguageId::JavaScript,
            LanguageId::Jsx,
            LanguageId::TypeScript,
            LanguageId::Tsx,
        ] {
            let result = parse(language, b"export const work=()=>1; export const other=function(){return 2}; const privateWork=()=>3;");
            for name in ["work", "other"] {
                assert!(
                    result.declarations.iter().any(|item| item.spelling == name
                        && item
                            .attributes
                            .iter()
                            .any(|attribute| attribute == "exported")),
                    "{language:?}: {name}"
                );
            }
            assert!(
                result
                    .declarations
                    .iter()
                    .filter(|item| item.spelling == "privateWork")
                    .all(|item| !item
                        .attributes
                        .iter()
                        .any(|attribute| attribute == "exported"))
            );
        }
    }

    #[test]
    fn registry_loads_all_dialects_and_compiles_embedded_queries() {
        let records = LanguageRegistry::validate_all().expect("registry should validate");
        assert_eq!(records.len(), LanguageId::ALL.len());
        assert!(
            records
                .iter()
                .all(|record| record.capabilities.parser_ready)
        );
        assert!(records.iter().all(|record| {
            record.capabilities.extractor_ready
                == matches!(
                    record.language,
                    LanguageId::Dart
                        | LanguageId::CSharp
                        | LanguageId::Rust
                        | LanguageId::Go
                        | LanguageId::Java
                        | LanguageId::JavaScript
                        | LanguageId::Jsx
                        | LanguageId::TypeScript
                        | LanguageId::Tsx
                )
        }));
        assert!(records.iter().all(|record| record.query_count > 0));
    }

    #[test]
    fn dialect_detection_is_explicit() {
        for path in ["x.js", "x.mjs", "x.cjs"] {
            assert!(matches!(
                LanguageId::for_path(Path::new(path)),
                Ok(LanguageId::JavaScript)
            ));
        }
        assert!(matches!(
            LanguageId::for_path(Path::new("x.jsx")),
            Ok(LanguageId::Jsx)
        ));
        for path in ["x.ts", "x.mts", "x.cts", "x.d.ts"] {
            assert!(matches!(
                LanguageId::for_path(Path::new(path)),
                Ok(LanguageId::TypeScript)
            ));
        }
        assert!(matches!(
            LanguageId::for_path(Path::new("x.tsx")),
            Ok(LanguageId::Tsx)
        ));
        assert_ne!(
            LanguageRegistry::provider(LanguageId::TypeScript).grammar_fingerprint(),
            LanguageRegistry::provider(LanguageId::Tsx).grammar_fingerprint()
        );
        assert!(LanguageId::for_path(Path::new("x.csx")).is_err());
    }

    #[test]
    fn invalid_queries_report_language_query_and_position() {
        let provider = LanguageRegistry::provider(LanguageId::Rust);
        let error = compile_query_with_context(
            LanguageId::Rust,
            &provider.language(),
            "rust/broken.scm",
            "(definitely_not_a_node) @declaration.test.name",
        )
        .expect_err("invalid node must fail query compilation");
        let text = error.to_string();
        assert!(text.contains("rust/broken.scm"));
        assert!(text.contains("rust"));
        assert!(text.contains("at 1:"));
    }

    #[test]
    fn ranges_preserve_unicode_crlf_and_bom_bytes() {
        let source = b"\xef\xbb\xbf// \xc4\xb0stanbul\r\nexport function selam(): void {}\r\n";
        let result = parse(LanguageId::TypeScript, source);
        assert!(!result.coverage.root_has_error);
        let selam = result
            .declarations
            .iter()
            .find(|item| item.spelling == "selam")
            .expect("selam declaration");
        assert_eq!(
            &source[selam.range.bytes.start() as usize..selam.range.bytes.end() as usize],
            b"selam"
        );
        assert_eq!(selam.range.start.line, 2);
        assert_eq!(selam.range.start.byte_column, 16);
    }

    #[test]
    fn empty_incomplete_and_invalid_encoding_are_distinct() {
        let empty = parse(LanguageId::Rust, b"");
        assert!(!empty.coverage.root_has_error);
        let incomplete = parse(
            LanguageId::TypeScript,
            b"export function unfinished(value: {",
        );
        assert!(incomplete.coverage.root_has_error);
        assert!(
            incomplete
                .diagnostics
                .iter()
                .any(|item| item.code == "syntax_error" || item.code == "syntax_missing")
        );
        let error = ParserWorker::new()
            .parse(
                LanguageId::Rust,
                &[0xff],
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect_err("invalid UTF-8 must be rejected");
        assert!(matches!(
            error,
            LanguageError::InvalidUtf8 { valid_up_to: 0 }
        ));
    }

    #[test]
    fn source_and_progress_work_budgets_fail_explicitly() {
        let source_limit = ParseLimits {
            max_source_bytes: 4,
            ..ParseLimits::default()
        };
        let source_error = ParserWorker::new()
            .parse(
                LanguageId::Rust,
                b"fn too_large() {}",
                source_limit,
                &CancellationContext::default(),
            )
            .expect_err("oversized source must fail before native parsing");
        assert!(matches!(source_error, LanguageError::SourceTooLarge { .. }));

        let mut deep_source = String::from("fn deep() { let _x = ");
        deep_source.push_str(&"(".repeat(40_000));
        deep_source.push('0');
        deep_source.push_str(&")".repeat(40_000));
        deep_source.push_str("; }");
        let work_limit = ParseLimits {
            max_progress_callbacks: 1,
            ..ParseLimits::default()
        };
        let work_error = ParserWorker::new()
            .parse(
                LanguageId::Rust,
                deep_source.as_bytes(),
                work_limit,
                &CancellationContext::default(),
            )
            .expect_err("native progress budget must stop deep parsing");
        assert!(matches!(
            work_error,
            LanguageError::WorkBudgetExceeded {
                stage: ParseStage::Parse
            }
        ));
    }

    #[test]
    fn capture_and_traversal_limits_are_observable() {
        let source = b"const a=1,b=2,c=3,d=4;";
        let limits = ParseLimits {
            max_captures: 2,
            max_syntax_nodes: 3,
            ..ParseLimits::default()
        };
        let result = ParserWorker::new()
            .parse(
                LanguageId::JavaScript,
                source,
                limits,
                &CancellationContext::default(),
            )
            .expect("bounded parse should still return partial observations");
        assert!(result.coverage.captures_truncated);
        assert!(result.coverage.syntax_traversal_truncated);
        assert_eq!(result.declarations.len(), 1);
        assert_eq!(result.scopes.len(), 1);
    }

    #[test]
    fn cancellation_is_cooperative_and_worker_can_be_reused() {
        let mut worker = ParserWorker::new();
        let cancellation = CancellationContext::default();
        cancellation.cancel();
        let error = worker
            .parse(
                LanguageId::Rust,
                b"fn cancelled() {}",
                ParseLimits::default(),
                &cancellation,
            )
            .expect_err("pre-cancelled work must stop");
        assert!(matches!(error, LanguageError::Cancelled { .. }));
        let result = worker
            .parse(
                LanguageId::Rust,
                b"fn recovered() {}",
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("worker must remain usable");
        assert!(
            result
                .declarations
                .iter()
                .any(|item| item.spelling == "recovered")
        );
    }

    #[test]
    fn progress_hook_can_stop_native_work_and_parser_is_reset() {
        let mut source = String::from("fn deep() { let _x = ");
        source.push_str(&"(".repeat(40_000));
        source.push('0');
        source.push_str(&")".repeat(40_000));
        source.push_str("; }");
        let mut worker = ParserWorker::new();
        let mut callbacks = 0_usize;
        let error = worker
            .parse_with_progress(
                LanguageId::Rust,
                source.as_bytes(),
                ParseLimits::default(),
                &CancellationContext::default(),
                &mut |_| {
                    callbacks += 1;
                    ControlFlow::Break(())
                },
            )
            .expect_err("observer should stop deep parse");
        assert!(matches!(error, LanguageError::ProgressStopped { .. }));
        assert!(callbacks > 0);
        let recovered = worker
            .parse(
                LanguageId::Rust,
                b"fn recovered() {}",
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("reset parser must not resume cancelled input");
        assert!(!recovered.coverage.root_has_error);
    }

    #[test]
    fn results_own_source_spelling_and_are_deterministic() {
        let mut worker = ParserWorker::new();
        let source = b"pub struct Owned; pub fn value() {}".to_vec();
        let first = worker
            .parse(
                LanguageId::Rust,
                &source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("first parse");
        drop(source);
        let second = worker
            .parse(
                LanguageId::Rust,
                b"pub struct Owned; pub fn value() {}",
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("second parse");
        assert_eq!(first, second);
        assert_eq!(first.declarations[0].spelling, "Owned");
    }

    #[test]
    fn rust_and_go_declaration_ids_survive_line_insertion() {
        for (language, path, source, shifted, names) in [
            (
                LanguageId::Rust,
                Path::new("src/domain.rs"),
                b"struct Invoice; impl Invoice { fn total(&self) {} }".as_slice(),
                b"\nstruct Invoice; impl Invoice { fn total(&self) {} }".as_slice(),
                ["Invoice", "total"],
            ),
            (
                LanguageId::Go,
                Path::new("domain.go"),
                b"package billing\ntype Invoice struct{}\nfunc (Invoice) Total() {}".as_slice(),
                b"\npackage billing\ntype Invoice struct{}\nfunc (Invoice) Total() {}".as_slice(),
                ["Invoice", "Total"],
            ),
        ] {
            let mut worker = ParserWorker::new();
            let original = worker
                .parse_file(
                    language,
                    path,
                    source,
                    ParseLimits::default(),
                    &CancellationContext::default(),
                )
                .expect("original source");
            let shifted = worker
                .parse_file(
                    language,
                    path,
                    shifted,
                    ParseLimits::default(),
                    &CancellationContext::default(),
                )
                .expect("line-shifted source");
            for name in names {
                let original_id = original
                    .declarations
                    .iter()
                    .find(|item| item.spelling == name)
                    .map(|item| item.id.as_str());
                let shifted_id = shifted
                    .declarations
                    .iter()
                    .find(|item| item.spelling == name)
                    .map(|item| item.id.as_str());
                assert_eq!(original_id, shifted_id, "stable id for {name}");
            }
        }
    }

    #[test]
    fn shadowed_bindings_and_same_name_receiver_methods_remain_distinct() {
        let rust = parse(
            LanguageId::Rust,
            b"struct A; struct B; impl A { fn run(&self) {} } impl B { fn run(&self) {} } fn f(value:i32) { let value=value; { let value=value; } }",
        );
        let rust_methods = rust
            .declarations
            .iter()
            .filter(|item| item.kind == "method" && item.spelling == "run")
            .collect::<Vec<_>>();
        assert_eq!(rust_methods.len(), 2);
        assert_ne!(rust_methods[0].id, rust_methods[1].id);
        assert_ne!(rust_methods[0].container, rust_methods[1].container);
        let rust_locals = rust
            .declarations
            .iter()
            .filter(|item| item.kind == "local" && item.spelling == "value")
            .collect::<Vec<_>>();
        assert_eq!(rust_locals.len(), 2);
        assert_ne!(rust_locals[0].id, rust_locals[1].id);
        assert_ne!(rust_locals[0].scope_id, rust_locals[1].scope_id);

        let go = parse(
            LanguageId::Go,
            b"package p\ntype A struct{}\ntype B struct{}\nfunc (A) Run() {}\nfunc (B) Run() {}",
        );
        let go_methods = go
            .declarations
            .iter()
            .filter(|item| item.kind == "method" && item.spelling == "Run")
            .collect::<Vec<_>>();
        assert_eq!(go_methods.len(), 2);
        assert_ne!(go_methods[0].id, go_methods[1].id);
        assert_eq!(go_methods[0].receiver_type.as_deref(), Some("A"));
        assert_eq!(go_methods[1].receiver_type.as_deref(), Some("B"));
    }

    #[test]
    fn rust_and_go_calls_are_explicitly_unresolved() {
        for (language, source) in [
            (LanguageId::Rust, b"fn f() { external::run(); }".as_slice()),
            (
                LanguageId::Go,
                b"package p\nfunc f() { external.Run() }".as_slice(),
            ),
        ] {
            let result = parse(language, source);
            assert!(!result.call_sites.is_empty());
            assert!(result.call_sites.iter().all(|call| {
                call.resolution == crate::ResolutionCategory::Unresolved
                    && call.target_id.is_none()
                    && call.scope_id.is_some()
            }));
        }
    }

    #[test]
    fn typescript_type_and_value_roles_remain_distinct() {
        let result = parse(
            LanguageId::TypeScript,
            b"interface Token<T> { read(): T } const Token = <T>(value: T): Token<T> => ({ read: () => value });",
        );
        let tokens = result
            .declarations
            .iter()
            .filter(|declaration| declaration.spelling == "Token")
            .collect::<Vec<_>>();
        assert_eq!(tokens.len(), 2);
        assert_ne!(tokens[0].id, tokens[1].id);
        assert!(
            tokens
                .iter()
                .any(|declaration| declaration.attributes.contains(&"role:type".to_owned()))
        );
        assert!(
            tokens
                .iter()
                .any(|declaration| declaration.attributes.contains(&"role:value".to_owned()))
        );
    }

    #[test]
    fn jsx_tags_and_dynamic_calls_are_not_overclaimed() {
        let result = parse(
            LanguageId::Tsx,
            b"const Card = <T,>({value}: {value:T}) => <Panel.Item value={value} />; object[key](); import(moduleName);",
        );
        let jsx = result
            .references
            .iter()
            .find(|reference| reference.kind == "jsx_component")
            .expect("JSX component reference");
        assert_eq!(jsx.spelling, "Panel.Item");
        assert!(
            jsx.limitations
                .contains(&"jsx_tag_not_a_call_edge".to_owned())
        );
        assert!(result.call_sites.iter().all(|call| {
            call.resolution == crate::ResolutionCategory::Unresolved && call.target_id.is_none()
        }));
        assert!(result.call_sites.iter().any(|call| {
            call.spelling == "object[key]"
                && call
                    .limitations
                    .contains(&"dynamic_property_target_not_resolved".to_owned())
        }));
        assert!(result.call_sites.iter().any(|call| {
            call.kind == "dynamic_import"
                && call
                    .limitations
                    .contains(&"dynamic_module_specifier_not_resolved".to_owned())
        }));
        assert!(
            result
                .call_sites
                .iter()
                .all(|call| call.spelling != "Panel.Item")
        );
    }

    #[test]
    fn js_ts_dialects_honor_cooperative_cancellation() {
        for language in [
            LanguageId::JavaScript,
            LanguageId::Jsx,
            LanguageId::TypeScript,
            LanguageId::Tsx,
        ] {
            let cancellation = CancellationContext::default();
            cancellation.cancel();
            let error = ParserWorker::new()
                .parse(
                    language,
                    b"const value = () => value();",
                    ParseLimits::default(),
                    &cancellation,
                )
                .expect_err("pre-cancelled dialect work must stop");
            assert!(matches!(error, LanguageError::Cancelled { .. }));
        }
    }

    #[test]
    fn dart_constructor_import_and_part_identities_remain_distinct() {
        let source = b"library billing.feature; import 'package:external/repo.dart' as repo; export 'model.dart'; part 'model_part.dart'; class Invoice { Invoice(); Invoice.named(); factory Invoice.from() = Invoice.named; int get total => 0; set total(int value) {} }";
        let result = ParserWorker::new()
            .parse_file(
                LanguageId::Dart,
                Path::new("lib/model.dart"),
                source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("Dart source");
        assert!(!result.coverage.root_has_error);
        let constructors = result
            .declarations
            .iter()
            .filter(|item| item.kind.contains("constructor"))
            .collect::<Vec<_>>();
        assert_eq!(constructors.len(), 3);
        assert_eq!(
            constructors
                .iter()
                .map(|item| item.spelling.as_str())
                .collect::<Vec<_>>(),
            ["Invoice", "named", "from"]
        );
        assert_eq!(
            constructors
                .iter()
                .map(|item| item.id.as_str())
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        assert!(result.imports.iter().any(|item| {
            item.kind == "import"
                && item.spelling == "package:external/repo.dart"
                && item.alias.as_deref() == Some("repo")
        }));
        assert!(result.imports.iter().any(|item| item.kind == "export"));
        assert!(result.imports.iter().any(|item| item.kind == "part"));
        let accessor_ids = result
            .declarations
            .iter()
            .filter(|item| item.spelling == "total")
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(accessor_ids.len(), 2);
        assert_ne!(accessor_ids[0], accessor_ids[1]);

        let part = parse(
            LanguageId::Dart,
            b"part of billing.feature; String label() => 'ok';",
        );
        assert!(part.imports.iter().any(|item| {
            item.kind == "part_of"
                && item.spelling == "billing.feature"
                && item
                    .limitations
                    .contains(&"part_relationship_not_resolved".to_owned())
        }));
    }

    #[test]
    fn dart_shadowing_prefixes_and_calls_stay_unresolved() {
        let result = parse(
            LanguageId::Dart,
            b"import 'math.dart' as calc; int twice(int value)=>value*2; int shadow(int Function(int) twice)=>twice(calc.twice(2));",
        );
        let declarations = result
            .declarations
            .iter()
            .filter(|item| item.spelling == "twice")
            .collect::<Vec<_>>();
        assert_eq!(declarations.len(), 2);
        assert_ne!(declarations[0].id, declarations[1].id);
        assert_ne!(declarations[0].scope_id, declarations[1].scope_id);
        assert!(result.call_sites.iter().all(|call| {
            call.resolution == crate::ResolutionCategory::Unresolved
                && call.target_id.is_none()
                && call.scope_id.is_some()
        }));
        let prefixed = result
            .call_sites
            .iter()
            .find(|call| call.spelling == "twice" && call.receiver_type.as_deref() == Some("calc"))
            .expect("prefixed call");
        assert!(
            prefixed
                .limitations
                .contains(&"receiver_type_or_import_prefix_not_inferred".to_owned())
        );
    }

    #[test]
    fn dart_modern_and_flutter_style_syntax_is_not_overclaimed() {
        let modern = parse(
            LanguageId::Dart,
            b"sealed class Outcome {} extension type InvoiceId(int value) {} String read((int,String) pair) => switch(pair) { (final amount, final currency) => currency };",
        );
        assert!(!modern.coverage.root_has_error);
        assert!(
            modern
                .declarations
                .iter()
                .any(|item| { item.kind == "extension_type" && item.spelling == "InvoiceId" })
        );
        assert_eq!(
            modern
                .declarations
                .iter()
                .filter(|item| item.kind == "pattern_variable")
                .count(),
            2
        );

        let flutter = parse(
            LanguageId::Dart,
            b"class Card { Widget build() => Column(children: [Text('x'), Button(onTap: () => save())]); }",
        );
        for name in ["Column", "Text", "Button"] {
            let call = flutter
                .call_sites
                .iter()
                .find(|item| item.spelling == name)
                .expect("nested Flutter-style constructor syntax");
            assert_eq!(call.kind, "constructor_like");
            assert!(call.target_id.is_none());
            assert!(
                call.limitations
                    .contains(&"flutter_widget_identity_and_tree_not_inferred".to_owned())
            );
        }
    }

    #[test]
    fn dart_preserves_unicode_crlf_bytes_and_resets_after_cancellation() {
        let source =
            "// İstanbul müşteri\r\nclass Customer { String label() => 'çağrı'; }\r\n".as_bytes();
        let result = parse(LanguageId::Dart, source);
        assert!(!result.coverage.root_has_error);
        let declaration = result
            .declarations
            .iter()
            .find(|item| item.spelling == "Customer")
            .expect("ASCII declaration after multibyte source");
        assert_eq!(
            &source
                [declaration.range.bytes.start() as usize..declaration.range.bytes.end() as usize],
            b"Customer"
        );
        assert_eq!(declaration.range.start.line, 2);

        let unsupported_identifier = parse(LanguageId::Dart, "class Müşteri {}".as_bytes());
        assert!(unsupported_identifier.coverage.root_has_error);

        let mut worker = ParserWorker::new();
        let cancellation = CancellationContext::default();
        cancellation.cancel();
        let error = worker
            .parse(
                LanguageId::Dart,
                source,
                ParseLimits::default(),
                &cancellation,
            )
            .expect_err("pre-cancelled Dart work must stop");
        assert!(matches!(error, LanguageError::Cancelled { .. }));
        let recovered = worker
            .parse(
                LanguageId::Dart,
                b"class Recovered {}",
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("Dart worker should remain reusable");
        assert!(!recovered.coverage.root_has_error);
    }

    #[test]
    fn csharp_partial_overloads_and_nested_types_remain_distinct() {
        let mut worker = ParserWorker::new();
        let first = worker
            .parse_file(
                LanguageId::CSharp,
                Path::new("Invoice.cs"),
                b"namespace Billing; public partial class Invoice { public int Total()=>0; public int Total(int value)=>value; public class Nested {} }",
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("first partial declaration");
        let second = worker
            .parse_file(
                LanguageId::CSharp,
                Path::new("Invoice.Metadata.cs"),
                b"namespace Billing; public partial class Invoice { public string Label { get; init; } }",
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .expect("second partial declaration");
        let first_partial = first
            .declarations
            .iter()
            .find(|item| item.kind == "class" && item.spelling == "Invoice")
            .expect("first Invoice declaration");
        let second_partial = second
            .declarations
            .iter()
            .find(|item| item.kind == "class" && item.spelling == "Invoice")
            .expect("second Invoice declaration");
        assert_ne!(first_partial.id, second_partial.id);
        assert!(
            first_partial
                .attributes
                .contains(&"partial_declaration".to_owned())
        );
        assert!(
            first_partial
                .attributes
                .contains(&"partial_group_hint:Billing.Invoice".to_owned())
        );
        assert_eq!(first_partial.attributes, second_partial.attributes);

        let overloads = first
            .declarations
            .iter()
            .filter(|item| item.kind == "method" && item.spelling == "Total")
            .collect::<Vec<_>>();
        assert_eq!(overloads.len(), 2);
        assert_ne!(overloads[0].id, overloads[1].id);
        assert_ne!(overloads[0].signature, overloads[1].signature);
        let nested = first
            .declarations
            .iter()
            .find(|item| item.kind == "class" && item.spelling == "Nested")
            .expect("nested class");
        assert_eq!(nested.container.as_deref(), Some("Invoice"));
    }

    #[test]
    fn csharp_and_java_dispatch_evidence_stays_unresolved() {
        for (language, source, receiver, limitation) in [
            (
                LanguageId::CSharp,
                b"interface IRepository { void Save(); } class Workflow { void Run(IRepository repository) { repository.Save(); var route = \"IRepository.Save\"; } }".as_slice(),
                "repository",
                "interface_virtual_or_extension_dispatch_not_resolved",
            ),
            (
                LanguageId::Java,
                b"interface Repository { void save(); } class Workflow { void run(Repository repository) { repository.save(); String route = \"Repository.save\"; } }".as_slice(),
                "repository",
                "interface_or_virtual_dispatch_not_resolved",
            ),
        ] {
            let result = parse(language, source);
            let call = result
                .call_sites
                .iter()
                .find(|item| matches!(item.spelling.as_str(), "Save" | "save"))
                .expect("interface-shaped call site");
            assert_eq!(call.receiver_type.as_deref(), Some(receiver));
            assert_eq!(call.resolution, crate::ResolutionCategory::Unresolved);
            assert!(call.target_id.is_none());
            assert!(call.limitations.contains(&limitation.to_owned()));
            assert!(result.call_sites.iter().all(|item| {
                item.spelling != "IRepository.Save" && item.spelling != "Repository.save"
            }));
        }
    }

    #[test]
    fn csharp_extension_and_java_method_reference_are_syntax_only() {
        let csharp = parse(
            LanguageId::CSharp,
            b"static class Extensions { public static int Twice(this int value) => value * 2; } class Use { int Run(int value) => value.Twice(); }",
        );
        let extension = csharp
            .declarations
            .iter()
            .find(|item| item.spelling == "Twice")
            .expect("extension method declaration");
        assert!(
            extension
                .attributes
                .contains(&"extension_method_syntax".to_owned())
        );
        let extension_call = csharp
            .call_sites
            .iter()
            .find(|item| item.spelling == "Twice")
            .expect("extension-shaped call");
        assert!(
            extension_call
                .limitations
                .contains(&"interface_virtual_or_extension_dispatch_not_resolved".to_owned())
        );

        let java = parse(
            LanguageId::Java,
            b"import java.util.function.Function; @interface Mark { String value(); } @Mark(\"x\") class Use { <T> T map(T value) { Function<T,T> f = Use::identity; return f.apply(value); } static <T> T identity(T value) { return value; } }",
        );
        let method_reference = java
            .references
            .iter()
            .find(|item| item.kind == "method_reference")
            .expect("method reference observation");
        assert_eq!(method_reference.spelling, "Use::identity");
        assert!(
            method_reference
                .limitations
                .contains(&"method_reference_not_a_call_edge".to_owned())
        );
        assert!(
            java.references
                .iter()
                .any(|item| item.kind == "annotation" && item.spelling == "Mark")
        );
    }

    #[test]
    fn csharp_and_java_preserve_unicode_crlf_ranges_and_cancellation() {
        for (language, source, name) in [
            (
                LanguageId::CSharp,
                "namespace Billing;\r\nclass Müşteri {}\r\n".as_bytes(),
                "Müşteri",
            ),
            (
                LanguageId::Java,
                "package billing;\r\nclass Müşteri {}\r\n".as_bytes(),
                "Müşteri",
            ),
        ] {
            let result = parse(language, source);
            let declaration = result
                .declarations
                .iter()
                .find(|item| item.spelling == name)
                .expect("Unicode declaration");
            assert_eq!(
                &source[declaration.range.bytes.start() as usize
                    ..declaration.range.bytes.end() as usize],
                name.as_bytes()
            );
            assert_eq!(declaration.range.start.line, 2);

            let cancellation = CancellationContext::default();
            cancellation.cancel();
            let error = ParserWorker::new()
                .parse(language, source, ParseLimits::default(), &cancellation)
                .expect_err("pre-cancelled language work must stop");
            assert!(matches!(error, LanguageError::Cancelled { .. }));
        }
    }
}
