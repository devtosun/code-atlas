#![no_main]

use ca_core::CancellationContext;
use ca_languages::{LanguageId, ParseLimits, ParserWorker};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&selector, source)) = data.split_first() else {
        return;
    };
    let language = match selector % 9 {
        0 => LanguageId::Dart,
        1 => LanguageId::CSharp,
        2 => LanguageId::Rust,
        3 => LanguageId::Go,
        4 => LanguageId::Java,
        5 => LanguageId::JavaScript,
        6 => LanguageId::Jsx,
        7 => LanguageId::TypeScript,
        _ => LanguageId::Tsx,
    };
    let source = &source[..source.len().min(256 * 1_024)];
    let mut worker = ParserWorker::new();
    let _ = worker.parse(
        language,
        source,
        ParseLimits::default(),
        &CancellationContext::default(),
    );
});
