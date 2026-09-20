use std::{fmt, path::Path, str::FromStr};

use ca_core::ByteRange;

use crate::LanguageError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LanguageId {
    Dart,
    CSharp,
    Rust,
    Go,
    Java,
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
}

impl LanguageId {
    pub const ALL: [Self; 9] = [
        Self::Dart,
        Self::CSharp,
        Self::Rust,
        Self::Go,
        Self::Java,
        Self::JavaScript,
        Self::Jsx,
        Self::TypeScript,
        Self::Tsx,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dart => "dart",
            Self::CSharp => "csharp",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Java => "java",
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
        }
    }

    pub fn for_path(path: &Path) -> Result<Self, LanguageError> {
        let path_text = path.to_string_lossy();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let language = match extension {
            "dart" => Self::Dart,
            "cs" => Self::CSharp,
            "rs" => Self::Rust,
            "go" => Self::Go,
            "java" => Self::Java,
            "jsx" => Self::Jsx,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "tsx" => Self::Tsx,
            "ts" | "mts" | "cts" => Self::TypeScript,
            _ if file_name.ends_with(".d.ts") => Self::TypeScript,
            _ => {
                return Err(LanguageError::UnsupportedPath {
                    path: path_text.into_owned(),
                });
            }
        };
        Ok(language)
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LanguageId {
    type Err = LanguageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let language = match value {
            "dart" => Self::Dart,
            "csharp" | "c#" => Self::CSharp,
            "rust" => Self::Rust,
            "go" => Self::Go,
            "java" => Self::Java,
            "javascript" | "js" => Self::JavaScript,
            "jsx" => Self::Jsx,
            "typescript" | "ts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            _ => {
                return Err(LanguageError::UnsupportedLanguageLabel {
                    label: value.to_owned(),
                });
            }
        };
        Ok(language)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtractionCapabilities {
    pub parser_ready: bool,
    pub extractor_ready: bool,
    pub declarations_ready: bool,
    pub scopes_ready: bool,
    pub imports_ready: bool,
    pub references_ready: bool,
    pub call_sites_ready: bool,
    pub name_resolution_ready: bool,
}

impl ExtractionCapabilities {
    pub const PARSER_KERNEL: Self = Self {
        parser_ready: true,
        extractor_ready: false,
        declarations_ready: false,
        scopes_ready: false,
        imports_ready: false,
        references_ready: false,
        call_sites_ready: false,
        name_resolution_ready: false,
    };

    pub const SOURCE_EXTRACTOR: Self = Self {
        parser_ready: true,
        extractor_ready: true,
        declarations_ready: true,
        scopes_ready: true,
        imports_ready: true,
        references_ready: true,
        call_sites_ready: true,
        name_resolution_ready: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourcePoint {
    /// One-based line number for human display.
    pub line: u32,
    /// Zero-based UTF-8 byte column.
    pub byte_column: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRange {
    /// Zero-based, half-open UTF-8 byte range in the original source bytes.
    pub bytes: ByteRange,
    pub start: SourcePoint,
    pub end: SourcePoint,
}

impl SourceRange {
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.bytes.start() <= other.bytes.start() && self.bytes.end() >= other.bytes.end()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionCategory {
    /// The occurrence is extracted from syntax, but no target binding is asserted.
    SyntaxObservation,
    /// The language layer explicitly leaves the target unresolved.
    Unresolved,
    /// A bounded lexical rule selected a declaration in the same source file.
    LexicallyResolved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxObservation {
    pub id: String,
    pub kind: String,
    pub spelling: String,
    pub range: SourceRange,
    pub syntax_range: SourceRange,
    pub scope_id: Option<String>,
    pub container: Option<String>,
    pub signature: Option<String>,
    pub receiver_type: Option<String>,
    pub alias: Option<String>,
    pub target_id: Option<String>,
    pub resolution: ResolutionCategory,
    pub attributes: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Information,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub range: Option<SourceRange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseCoverage {
    pub root_has_error: bool,
    pub syntax_nodes_visited: usize,
    pub syntax_traversal_truncated: bool,
    pub captures_truncated: bool,
    pub query_match_limit_exceeded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractionResult {
    pub language: LanguageId,
    pub source_hash: String,
    pub grammar_fingerprint: String,
    pub query_fingerprint: String,
    pub extractor_fingerprint: String,
    pub capabilities: ExtractionCapabilities,
    pub declarations: Vec<SyntaxObservation>,
    pub scopes: Vec<SyntaxObservation>,
    pub imports: Vec<SyntaxObservation>,
    pub references: Vec<SyntaxObservation>,
    pub call_sites: Vec<SyntaxObservation>,
    pub conditions: Vec<SyntaxObservation>,
    pub diagnostics: Vec<Diagnostic>,
    pub coverage: ParseCoverage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseStage {
    Parse,
    Query,
}

impl fmt::Display for ParseStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Parse => "parse",
            Self::Query => "query",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressEvent {
    pub stage: ParseStage,
    pub current_byte_offset: usize,
    pub callback_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseLimits {
    pub max_source_bytes: usize,
    pub max_captures: usize,
    pub max_syntax_nodes: usize,
    pub max_diagnostics: usize,
    pub max_progress_callbacks: usize,
    pub query_match_limit: u32,
}

impl ParseLimits {
    pub const DEFAULT: Self = Self {
        max_source_bytes: 2 * 1024 * 1024,
        max_captures: 20_000,
        max_syntax_nodes: 200_000,
        max_diagnostics: 256,
        max_progress_callbacks: 100_000,
        query_match_limit: 8_192,
    };

    pub(crate) fn validate(self) -> Result<(), LanguageError> {
        if self.max_source_bytes == 0 {
            return Err(LanguageError::InvalidLimits {
                detail: "max_source_bytes must be greater than zero",
            });
        }
        if self.max_captures == 0 {
            return Err(LanguageError::InvalidLimits {
                detail: "max_captures must be greater than zero",
            });
        }
        if self.max_syntax_nodes == 0 {
            return Err(LanguageError::InvalidLimits {
                detail: "max_syntax_nodes must be greater than zero",
            });
        }
        if self.max_diagnostics == 0 {
            return Err(LanguageError::InvalidLimits {
                detail: "max_diagnostics must be greater than zero",
            });
        }
        if self.max_progress_callbacks == 0 {
            return Err(LanguageError::InvalidLimits {
                detail: "max_progress_callbacks must be greater than zero",
            });
        }
        if !(1..=65_536).contains(&self.query_match_limit) {
            return Err(LanguageError::InvalidLimits {
                detail: "query_match_limit must be in 1..=65536",
            });
        }
        Ok(())
    }
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
