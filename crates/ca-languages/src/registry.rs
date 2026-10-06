use std::path::Path;

use tree_sitter::{Language, Parser, Query};

use crate::{ExtractionCapabilities, LanguageError, LanguageId};

#[derive(Clone, Copy, Debug)]
pub struct QueryAsset {
    pub name: &'static str,
    pub source: &'static str,
}

#[derive(Clone, Copy)]
pub struct LanguageProvider {
    pub id: LanguageId,
    pub package: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub extensions: &'static [&'static str],
    pub capabilities: ExtractionCapabilities,
    language: fn() -> Language,
    node_types: &'static str,
    queries: &'static [QueryAsset],
}

impl LanguageProvider {
    #[must_use]
    pub fn language(self) -> Language {
        (self.language)()
    }

    #[must_use]
    pub const fn queries(self) -> &'static [QueryAsset] {
        self.queries
    }

    #[must_use]
    pub fn grammar_fingerprint(self) -> String {
        let language = self.language();
        fingerprint(&[
            self.package.as_bytes(),
            self.version.as_bytes(),
            &language.abi_version().to_le_bytes(),
            self.node_types.as_bytes(),
        ])
    }

    #[must_use]
    pub fn query_fingerprint(self) -> String {
        let mut hasher = blake3::Hasher::new();
        for query in self.queries {
            hasher.update(query.name.as_bytes());
            hasher.update(&[0]);
            hasher.update(query.source.as_bytes());
            hasher.update(&[0xff]);
        }
        hasher.finalize().to_hex().to_string()
    }

    #[must_use]
    pub fn extractor_fingerprint(self) -> String {
        let extractor_version = match self.id {
            LanguageId::Dart => "dart-source-v2",
            LanguageId::CSharp => "csharp-source-v2",
            LanguageId::Rust => "rust-source-v2",
            LanguageId::Go => "go-source-v2",
            LanguageId::Java => "java-source-v2",
            LanguageId::JavaScript | LanguageId::Jsx => "javascript-source-v2",
            LanguageId::TypeScript | LanguageId::Tsx => "typescript-source-v2",
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.query_fingerprint().as_bytes());
        hasher.update(&[0]);
        hasher.update(extractor_version.as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

impl std::fmt::Debug for LanguageProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LanguageProvider")
            .field("id", &self.id)
            .field("package", &self.package)
            .field("version", &self.version)
            .field("extensions", &self.extensions)
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

fn fingerprint(parts: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

fn dart() -> Language {
    tree_sitter_dart::LANGUAGE.into()
}
fn csharp() -> Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}
fn rust() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}
fn go() -> Language {
    tree_sitter_go::LANGUAGE.into()
}
fn java() -> Language {
    tree_sitter_java::LANGUAGE.into()
}
fn javascript() -> Language {
    tree_sitter_javascript::LANGUAGE.into()
}
fn typescript() -> Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}
fn tsx() -> Language {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

const DART_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "dart/symbols.scm",
        source: include_str!("../queries/dart/symbols.scm"),
    },
    QueryAsset {
        name: "dart/imports.scm",
        source: include_str!("../queries/dart/imports.scm"),
    },
    QueryAsset {
        name: "dart/references.scm",
        source: include_str!("../queries/dart/references.scm"),
    },
    QueryAsset {
        name: "dart/calls.scm",
        source: include_str!("../queries/dart/calls.scm"),
    },
];
const CSHARP_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "csharp/symbols.scm",
        source: include_str!("../queries/csharp/symbols.scm"),
    },
    QueryAsset {
        name: "csharp/imports.scm",
        source: include_str!("../queries/csharp/imports.scm"),
    },
    QueryAsset {
        name: "csharp/references.scm",
        source: include_str!("../queries/csharp/references.scm"),
    },
    QueryAsset {
        name: "csharp/calls.scm",
        source: include_str!("../queries/csharp/calls.scm"),
    },
];
const RUST_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "rust/symbols.scm",
        source: include_str!("../queries/rust/symbols.scm"),
    },
    QueryAsset {
        name: "rust/imports.scm",
        source: include_str!("../queries/rust/imports.scm"),
    },
    QueryAsset {
        name: "rust/references.scm",
        source: include_str!("../queries/rust/references.scm"),
    },
    QueryAsset {
        name: "rust/calls.scm",
        source: include_str!("../queries/rust/calls.scm"),
    },
];
const GO_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "go/symbols.scm",
        source: include_str!("../queries/go/symbols.scm"),
    },
    QueryAsset {
        name: "go/imports.scm",
        source: include_str!("../queries/go/imports.scm"),
    },
    QueryAsset {
        name: "go/references.scm",
        source: include_str!("../queries/go/references.scm"),
    },
    QueryAsset {
        name: "go/calls.scm",
        source: include_str!("../queries/go/calls.scm"),
    },
];
const JAVA_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "java/symbols.scm",
        source: include_str!("../queries/java/symbols.scm"),
    },
    QueryAsset {
        name: "java/imports.scm",
        source: include_str!("../queries/java/imports.scm"),
    },
    QueryAsset {
        name: "java/references.scm",
        source: include_str!("../queries/java/references.scm"),
    },
    QueryAsset {
        name: "java/calls.scm",
        source: include_str!("../queries/java/calls.scm"),
    },
];
const JAVASCRIPT_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "javascript/symbols.scm",
        source: include_str!("../queries/javascript/symbols.scm"),
    },
    QueryAsset {
        name: "javascript/imports.scm",
        source: include_str!("../queries/javascript/imports.scm"),
    },
    QueryAsset {
        name: "javascript/references.scm",
        source: include_str!("../queries/javascript/references.scm"),
    },
    QueryAsset {
        name: "javascript/calls.scm",
        source: include_str!("../queries/javascript/calls.scm"),
    },
];
const JSX_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "jsx/symbols.scm",
        source: include_str!("../queries/jsx/symbols.scm"),
    },
    QueryAsset {
        name: "jsx/imports.scm",
        source: include_str!("../queries/jsx/imports.scm"),
    },
    QueryAsset {
        name: "jsx/references.scm",
        source: include_str!("../queries/jsx/references.scm"),
    },
    QueryAsset {
        name: "jsx/calls.scm",
        source: include_str!("../queries/jsx/calls.scm"),
    },
];
const TYPESCRIPT_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "typescript/symbols.scm",
        source: include_str!("../queries/typescript/symbols.scm"),
    },
    QueryAsset {
        name: "typescript/imports.scm",
        source: include_str!("../queries/typescript/imports.scm"),
    },
    QueryAsset {
        name: "typescript/references.scm",
        source: include_str!("../queries/typescript/references.scm"),
    },
    QueryAsset {
        name: "typescript/calls.scm",
        source: include_str!("../queries/typescript/calls.scm"),
    },
];
const TSX_QUERIES: &[QueryAsset] = &[
    QueryAsset {
        name: "tsx/symbols.scm",
        source: include_str!("../queries/tsx/symbols.scm"),
    },
    QueryAsset {
        name: "tsx/imports.scm",
        source: include_str!("../queries/tsx/imports.scm"),
    },
    QueryAsset {
        name: "tsx/references.scm",
        source: include_str!("../queries/tsx/references.scm"),
    },
    QueryAsset {
        name: "tsx/calls.scm",
        source: include_str!("../queries/tsx/calls.scm"),
    },
];

const SOURCE_CAPABILITIES: ExtractionCapabilities = ExtractionCapabilities::SOURCE_EXTRACTOR;

const PROVIDERS: [LanguageProvider; 9] = [
    LanguageProvider {
        id: LanguageId::Dart,
        package: "tree-sitter-dart",
        version: "0.2.0",
        license: "MIT",
        extensions: &["dart"],
        capabilities: SOURCE_CAPABILITIES,
        language: dart,
        node_types: tree_sitter_dart::NODE_TYPES,
        queries: DART_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::CSharp,
        package: "tree-sitter-c-sharp",
        version: "0.23.5",
        license: "MIT",
        extensions: &["cs"],
        capabilities: SOURCE_CAPABILITIES,
        language: csharp,
        node_types: tree_sitter_c_sharp::NODE_TYPES,
        queries: CSHARP_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::Rust,
        package: "tree-sitter-rust",
        version: "0.24.2",
        license: "MIT",
        extensions: &["rs"],
        capabilities: SOURCE_CAPABILITIES,
        language: rust,
        node_types: tree_sitter_rust::NODE_TYPES,
        queries: RUST_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::Go,
        package: "tree-sitter-go",
        version: "0.25.0",
        license: "MIT",
        extensions: &["go"],
        capabilities: SOURCE_CAPABILITIES,
        language: go,
        node_types: tree_sitter_go::NODE_TYPES,
        queries: GO_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::Java,
        package: "tree-sitter-java",
        version: "0.23.5",
        license: "MIT",
        extensions: &["java"],
        capabilities: SOURCE_CAPABILITIES,
        language: java,
        node_types: tree_sitter_java::NODE_TYPES,
        queries: JAVA_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::JavaScript,
        package: "tree-sitter-javascript",
        version: "0.25.0",
        license: "MIT",
        extensions: &["js", "mjs", "cjs"],
        capabilities: SOURCE_CAPABILITIES,
        language: javascript,
        node_types: tree_sitter_javascript::NODE_TYPES,
        queries: JAVASCRIPT_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::Jsx,
        package: "tree-sitter-javascript",
        version: "0.25.0",
        license: "MIT",
        extensions: &["jsx"],
        capabilities: SOURCE_CAPABILITIES,
        language: javascript,
        node_types: tree_sitter_javascript::NODE_TYPES,
        queries: JSX_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::TypeScript,
        package: "tree-sitter-typescript",
        version: "0.23.2",
        license: "MIT",
        extensions: &["ts", "mts", "cts", "d.ts"],
        capabilities: SOURCE_CAPABILITIES,
        language: typescript,
        node_types: tree_sitter_typescript::TYPESCRIPT_NODE_TYPES,
        queries: TYPESCRIPT_QUERIES,
    },
    LanguageProvider {
        id: LanguageId::Tsx,
        package: "tree-sitter-typescript",
        version: "0.23.2",
        license: "MIT",
        extensions: &["tsx"],
        capabilities: SOURCE_CAPABILITIES,
        language: tsx,
        node_types: tree_sitter_typescript::TSX_NODE_TYPES,
        queries: TSX_QUERIES,
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrammarValidation {
    pub language: LanguageId,
    pub package: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub abi_version: usize,
    pub grammar_fingerprint: String,
    pub query_fingerprint: String,
    pub query_count: usize,
    pub capabilities: ExtractionCapabilities,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LanguageRegistry;

impl LanguageRegistry {
    #[must_use]
    pub const fn providers() -> &'static [LanguageProvider] {
        &PROVIDERS
    }

    #[must_use]
    pub fn provider(language: LanguageId) -> LanguageProvider {
        PROVIDERS[language_index(language)]
    }

    pub fn detect(path: &Path) -> Result<LanguageProvider, LanguageError> {
        LanguageId::for_path(path).map(Self::provider)
    }

    pub fn validate_all() -> Result<Vec<GrammarValidation>, LanguageError> {
        PROVIDERS.iter().copied().map(validate_provider).collect()
    }
}

const fn language_index(language: LanguageId) -> usize {
    match language {
        LanguageId::Dart => 0,
        LanguageId::CSharp => 1,
        LanguageId::Rust => 2,
        LanguageId::Go => 3,
        LanguageId::Java => 4,
        LanguageId::JavaScript => 5,
        LanguageId::Jsx => 6,
        LanguageId::TypeScript => 7,
        LanguageId::Tsx => 8,
    }
}

fn validate_provider(provider: LanguageProvider) -> Result<GrammarValidation, LanguageError> {
    let language = provider.language();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| LanguageError::GrammarLoad {
            language: provider.id,
            message: error.to_string(),
        })?;
    for asset in provider.queries() {
        let query = compile_query_with_context(provider.id, &language, asset.name, asset.source)?;
        validate_capture_names(provider.id, asset.name, &query)?;
    }
    Ok(GrammarValidation {
        language: provider.id,
        package: provider.package,
        version: provider.version,
        license: provider.license,
        abi_version: language.abi_version(),
        grammar_fingerprint: provider.grammar_fingerprint(),
        query_fingerprint: provider.query_fingerprint(),
        query_count: provider.queries().len(),
        capabilities: provider.capabilities,
    })
}

pub fn compile_query_with_context(
    language_id: LanguageId,
    language: &Language,
    query_name: &str,
    source: &str,
) -> Result<Query, LanguageError> {
    Query::new(language, source).map_err(|error| LanguageError::InvalidQuery {
        language: language_id,
        query_name: query_name.to_owned(),
        row: error.row + 1,
        column: error.column,
        message: error.message,
    })
}

pub(crate) fn validate_capture_names(
    language: LanguageId,
    query_name: &str,
    query: &Query,
) -> Result<(), LanguageError> {
    for capture in query.capture_names() {
        let mut parts = capture.split('.');
        let category = parts.next();
        let kind = parts.next();
        let role = parts.next();
        let valid_category = matches!(
            category,
            Some("declaration" | "scope" | "import" | "reference" | "call" | "condition")
        );
        let valid_role = matches!(role, Some("name" | "node"));
        let valid = valid_category && kind.is_some() && valid_role && parts.next().is_none();
        if !valid {
            return Err(LanguageError::InvalidCaptureContract {
                language,
                query_name: query_name.to_owned(),
                capture: (*capture).to_owned(),
            });
        }
    }
    Ok(())
}
