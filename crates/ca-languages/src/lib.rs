#![forbid(unsafe_code)]
#![doc = "Bounded Tree-sitter parser and language-provider boundary for CodeAtlas."]
#![doc = ""]
#![doc = "All seven required languages plus JSX/TSX provide syntax-only declarations,"]
#![doc = "scopes, imports, references and unresolved call sites. No adapter claims compiler,"]
#![doc = "analyzer, package-manager, framework or runtime dispatch semantics."]

mod adapters;
mod model;
mod registry;
mod worker;

pub use model::{
    Diagnostic, DiagnosticSeverity, ExtractionCapabilities, ExtractionResult, LanguageId,
    ParseCoverage, ParseLimits, ParseStage, ProgressEvent, ResolutionCategory, SourcePoint,
    SourceRange, SyntaxObservation,
};
pub use registry::{
    GrammarValidation, LanguageProvider, LanguageRegistry, QueryAsset, compile_query_with_context,
};
pub use worker::ParserWorker;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LanguageError {
    #[error("unsupported language label: {label}")]
    UnsupportedLanguageLabel { label: String },
    #[error("no supported language/dialect for path: {path}")]
    UnsupportedPath { path: String },
    #[error("source has {actual} bytes, exceeding the configured limit of {limit}")]
    SourceTooLarge { actual: usize, limit: usize },
    #[error("source is not valid UTF-8 at byte {valid_up_to}")]
    InvalidUtf8 { valid_up_to: usize },
    #[error("invalid parse limits: {detail}")]
    InvalidLimits { detail: &'static str },
    #[error("operation cancelled during {stage}")]
    Cancelled { stage: ParseStage },
    #[error("cooperative work budget exhausted during {stage}")]
    WorkBudgetExceeded { stage: ParseStage },
    #[error("progress observer stopped work during {stage}")]
    ProgressStopped { stage: ParseStage },
    #[error("Tree-sitter returned no tree for {language} without a cancellation reason")]
    ParseFailed { language: LanguageId },
    #[error("could not load {language} grammar: {message}")]
    GrammarLoad {
        language: LanguageId,
        message: String,
    },
    #[error("invalid embedded query {query_name} for {language} at {row}:{column}: {message}")]
    InvalidQuery {
        language: LanguageId,
        query_name: String,
        row: usize,
        column: usize,
        message: String,
    },
    #[error("query {query_name} for {language} uses unsupported capture {capture}")]
    InvalidCaptureContract {
        language: LanguageId,
        query_name: String,
        capture: String,
    },
    #[error("source coordinate cannot be represented by the public range type")]
    CoordinateOverflow,
    #[error("parser worker did not retain initialized state for {language}")]
    WorkerStateMissing { language: LanguageId },
}
