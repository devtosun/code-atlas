#![forbid(unsafe_code)]

use std::{
    collections::HashSet,
    env, fs,
    path::{Component, Path, PathBuf},
    process::{Command, ExitCode},
};

use ca_core::CancellationContext;
use ca_languages::{
    ExtractionResult, LanguageId, LanguageRegistry, ParseLimits, ParserWorker, ResolutionCategory,
    SyntaxObservation,
};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Gate {
    program: &'static str,
    arguments: &'static [&'static str],
}

const VERIFY_GATES: &[Gate] = &[
    Gate {
        program: "cargo",
        arguments: &["fmt", "--all", "--", "--check"],
    },
    Gate {
        program: "cargo",
        arguments: &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    },
    Gate {
        program: "cargo",
        arguments: &["test", "--workspace", "--locked"],
    },
];

const DELIBERATE_FAILURE: Gate = Gate {
    program: "cargo",
    arguments: &[
        "check",
        "-p",
        "ca-core",
        "--features",
        "__codeatlas_deliberate_failure__",
    ],
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a direct workspace child")
        .to_owned()
}

fn run_gate(gate: Gate, root: &Path) -> Result<(), i32> {
    let program = if gate.program == "cargo" {
        env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
    } else {
        gate.program.into()
    };
    eprintln!("+ {} {}", gate.program, gate.arguments.join(" "));
    let status = Command::new(program)
        .args(gate.arguments)
        .current_dir(root)
        .status()
        .map_err(|error| {
            eprintln!("xtask could not start {}: {error}", gate.program);
            1
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(status.code().unwrap_or(1))
    }
}

fn verify(inject_failure: bool) -> Result<(), i32> {
    let root = workspace_root();
    for gate in VERIFY_GATES {
        run_gate(*gate, &root)?;
    }
    if inject_failure {
        run_gate(DELIBERATE_FAILURE, &root)?;
    }
    grammar_check()?;
    fixtures()?;
    rust_go_fixtures()?;
    js_ts_fixtures()?;
    csharp_java_fixtures()?;
    dart_fixtures()?;
    phase14_real_corpus()?;
    Ok(())
}

fn grammar_check() -> Result<(), i32> {
    let records = LanguageRegistry::validate_all().map_err(|error| {
        eprintln!("grammar-check failed: {error}");
        3
    })?;
    for record in records {
        println!(
            "language={} package={} version={} license={} abi={} parser_ready={} extractor_ready={} grammar_hash={} query_hash={} queries={}",
            record.language,
            record.package,
            record.version,
            record.license,
            record.abi_version,
            record.capabilities.parser_ready,
            record.capabilities.extractor_ready,
            record.grammar_fingerprint,
            record.query_fingerprint,
            record.query_count,
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct FixtureManifest {
    schema_version: u32,
    fixtures: Vec<FixtureExpectation>,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectation {
    id: String,
    file: String,
    language: String,
    required_declaration_names: Vec<String>,
    forbidden_declaration_names: Vec<String>,
    expected_parse: String,
    execution_status: String,
}

fn fixtures() -> Result<(), i32> {
    let root = workspace_root();
    let actual = fixture_snapshot(&root).map_err(|error| {
        eprintln!("fixtures failed: {error}");
        4
    })?;
    let golden_path = root.join("fixtures/parser-kernel.golden");
    let golden = fs::read_to_string(&golden_path).map_err(|error| {
        eprintln!(
            "fixtures failed: cannot read reviewed golden {}: {error}\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        4
    })?;
    if actual != golden {
        eprintln!(
            "fixtures failed: reviewed golden {} differs\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        return Err(4);
    }
    print!("{actual}");
    Ok(())
}

fn fixture_snapshot(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("fixtures/expectations.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: FixtureManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported fixture schema version {}",
            manifest.schema_version
        ));
    }
    let fixture_root = root
        .join("fixtures")
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize fixture root: {error}"))?;
    let mut worker = ParserWorker::new();
    let mut snapshot = String::new();
    for expected in &manifest.fixtures {
        if expected.execution_status != "PARSER_KERNEL_PASS" {
            return Err(format!(
                "fixture {} has stale execution_status {}",
                expected.id, expected.execution_status
            ));
        }
        let relative = Path::new(&expected.file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!("fixture {} has unsafe path", expected.id));
        }
        let path = root.join(relative);
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("cannot open fixture {}: {error}", path.display()))?;
        if !canonical.starts_with(&fixture_root) {
            return Err(format!("fixture {} escapes fixtures root", expected.id));
        }
        let source = fs::read(&canonical)
            .map_err(|error| format!("cannot read fixture {}: {error}", path.display()))?;
        let language = expected
            .language
            .parse::<LanguageId>()
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        let detected = LanguageRegistry::detect(relative)
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if detected.id != language {
            return Err(format!(
                "fixture {} labels {} but path detects {}",
                expected.id, language, detected.id
            ));
        }
        let result = worker
            .parse_file(
                language,
                relative,
                &source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        validate_parse_expectation(expected, &result)?;
        validate_names(expected, &result)?;
        append_snapshot_line(&mut snapshot, expected, &result);
    }
    Ok(snapshot)
}

#[derive(Debug, Deserialize)]
struct Phase14CorpusManifest {
    schema_version: u32,
    authorization: String,
    files: Vec<Phase14CorpusFile>,
}

#[derive(Debug, Deserialize)]
struct Phase14CorpusFile {
    file: String,
    language: String,
    sha256: String,
}

fn phase14_real_corpus() -> Result<(), i32> {
    let root = workspace_root();
    let actual = phase14_real_corpus_snapshot(&root).map_err(|error| {
        eprintln!("phase14-real-corpus failed: {error}");
        4
    })?;
    let golden_path = root.join("fixtures/phase14-real-corpus.golden");
    let golden = fs::read_to_string(&golden_path).map_err(|error| {
        eprintln!(
            "phase14-real-corpus failed: cannot read reviewed golden {}: {error}\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        4
    })?;
    if actual != golden {
        eprintln!(
            "phase14-real-corpus failed: reviewed golden {} differs\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        return Err(4);
    }
    print!("{actual}");
    Ok(())
}

fn phase14_real_corpus_snapshot(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("fixtures/phase14-real-corpus.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: Phase14CorpusManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported Phase 14 corpus schema version {}",
            manifest.schema_version
        ));
    }
    if manifest.authorization != "project-owned-source-authorized-for-local-evaluation" {
        return Err("Phase 14 corpus authorization is missing or unsupported".to_owned());
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize workspace root: {error}"))?;
    let mut worker = ParserWorker::new();
    let mut snapshot = String::new();
    for expected in &manifest.files {
        let relative = Path::new(&expected.file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!("corpus file {} has unsafe path", expected.file));
        }
        if expected.sha256.len() != 64
            || !expected
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!("corpus file {} has invalid SHA-256", expected.file));
        }
        let canonical = root
            .join(relative)
            .canonicalize()
            .map_err(|error| format!("cannot open corpus file {}: {error}", expected.file))?;
        if !canonical.starts_with(&canonical_root) {
            return Err(format!(
                "corpus file {} escapes workspace root",
                expected.file
            ));
        }
        let source = fs::read(&canonical)
            .map_err(|error| format!("cannot read corpus file {}: {error}", expected.file))?;
        let language = expected
            .language
            .parse::<LanguageId>()
            .map_err(|error| format!("corpus file {}: {error}", expected.file))?;
        let result = worker
            .parse_file(
                language,
                relative,
                &source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .map_err(|error| format!("corpus file {}: {error}", expected.file))?;
        let declarations = result
            .declarations
            .iter()
            .map(|item| {
                format!(
                    "{}:{}@{}-{}",
                    item.kind,
                    item.spelling,
                    item.range.bytes.start(),
                    item.range.bytes.end()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        snapshot.push_str(&format!(
            "file={} language={} bytes={} source_hash={} declarations={}\n",
            expected.file,
            result.language,
            source.len(),
            result.source_hash,
            declarations
        ));
    }
    Ok(snapshot)
}

#[derive(Debug, Deserialize)]
struct RustGoManifest {
    schema_version: u32,
    fixtures: Vec<RustGoExpectation>,
}

#[derive(Debug, Deserialize)]
struct RustGoExpectation {
    id: String,
    file: String,
    language: String,
    expected_parse: String,
    required_declarations: Vec<RequiredDeclaration>,
    required_imports: Vec<RequiredImport>,
    required_calls: Vec<RequiredCall>,
    required_conditions: Vec<String>,
    forbidden_observations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RequiredDeclaration {
    kind: String,
    name: String,
    container: Option<String>,
    receiver_type: Option<String>,
    signature_contains: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequiredImport {
    path: String,
    alias: Option<String>,
    limitation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequiredCall {
    kind: String,
    name: String,
    attribute: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsTsManifest {
    schema_version: u32,
    fixtures: Vec<JsTsExpectation>,
}

#[derive(Debug, Deserialize)]
struct JsTsExpectation {
    id: String,
    file: String,
    language: String,
    expected_parse: String,
    required_declarations: Vec<JsTsDeclaration>,
    required_imports: Vec<JsTsImport>,
    required_calls: Vec<JsTsOccurrence>,
    required_references: Vec<JsTsOccurrence>,
    forbidden_observations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct JsTsDeclaration {
    kind: String,
    name: String,
    attribute: Option<String>,
    signature_contains: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsTsImport {
    kind: String,
    path: String,
    alias: Option<String>,
    attribute: Option<String>,
    limitation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsTsOccurrence {
    kind: String,
    name: String,
    attribute: Option<String>,
    limitation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CSharpJavaManifest {
    schema_version: u32,
    fixtures: Vec<CSharpJavaExpectation>,
}

#[derive(Debug, Deserialize)]
struct CSharpJavaExpectation {
    id: String,
    file: String,
    language: String,
    expected_parse: String,
    required_declarations: Vec<CSharpJavaDeclaration>,
    required_imports: Vec<CSharpJavaImport>,
    required_calls: Vec<CSharpJavaOccurrence>,
    required_references: Vec<CSharpJavaOccurrence>,
    required_conditions: Vec<String>,
    forbidden_observations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CSharpJavaDeclaration {
    kind: String,
    name: String,
    container: Option<String>,
    attribute: Option<String>,
    limitation: Option<String>,
    signature_contains: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CSharpJavaImport {
    kind: String,
    path: String,
    alias: Option<String>,
    attribute: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CSharpJavaOccurrence {
    kind: String,
    name: String,
    receiver_expression: Option<String>,
    attribute: Option<String>,
    limitation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DartManifest {
    schema_version: u32,
    fixtures: Vec<DartExpectation>,
}

#[derive(Debug, Deserialize)]
struct DartExpectation {
    id: String,
    file: String,
    language: String,
    expected_parse: String,
    required_declarations: Vec<DartDeclaration>,
    required_imports: Vec<DartImport>,
    required_calls: Vec<DartOccurrence>,
    required_references: Vec<DartOccurrence>,
    required_diagnostic_fragments: Vec<String>,
    forbidden_observations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DartDeclaration {
    kind: String,
    name: String,
    container: Option<String>,
    attribute: Option<String>,
    limitation: Option<String>,
    signature_contains: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DartImport {
    kind: String,
    path: String,
    alias: Option<String>,
    attribute: Option<String>,
    limitation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DartOccurrence {
    kind: String,
    name: String,
    receiver_expression: Option<String>,
    attribute: Option<String>,
    limitation: Option<String>,
}

fn rust_go_fixtures() -> Result<(), i32> {
    let root = workspace_root();
    let actual = rust_go_snapshot(&root).map_err(|error| {
        eprintln!("rust-go-fixtures failed: {error}");
        5
    })?;
    let golden_path = root.join("fixtures/rust-go.golden");
    let golden = fs::read_to_string(&golden_path).map_err(|error| {
        eprintln!(
            "rust-go-fixtures failed: cannot read reviewed golden {}: {error}\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        5
    })?;
    if actual != golden {
        eprintln!(
            "rust-go-fixtures failed: reviewed golden {} differs\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        return Err(5);
    }
    print!("{actual}");
    Ok(())
}

fn rust_go_snapshot(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("fixtures/rust-go-expectations.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: RustGoManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported Rust/Go fixture schema version {}",
            manifest.schema_version
        ));
    }
    let fixture_root = root
        .join("fixtures")
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize fixture root: {error}"))?;
    let mut worker = ParserWorker::new();
    let mut snapshot = String::new();
    for expected in &manifest.fixtures {
        let relative = Path::new(&expected.file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!("fixture {} has unsafe path", expected.id));
        }
        let canonical = root
            .join(relative)
            .canonicalize()
            .map_err(|error| format!("cannot open fixture {}: {error}", expected.id))?;
        if !canonical.starts_with(&fixture_root) {
            return Err(format!("fixture {} escapes fixtures root", expected.id));
        }
        let source = fs::read(&canonical)
            .map_err(|error| format!("cannot read fixture {}: {error}", expected.id))?;
        let language = expected
            .language
            .parse::<LanguageId>()
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if !matches!(language, LanguageId::Rust | LanguageId::Go) {
            return Err(format!("fixture {} is not Rust or Go", expected.id));
        }
        let result = worker
            .parse_file(
                language,
                relative,
                &source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        validate_rust_go_fixture(expected, &source, &result)?;
        append_rust_go_snapshot(&mut snapshot, expected, &result);
    }
    Ok(snapshot)
}

fn validate_rust_go_fixture(
    expected: &RustGoExpectation,
    source: &[u8],
    result: &ExtractionResult,
) -> Result<(), String> {
    let parse_expectation = FixtureExpectation {
        id: expected.id.clone(),
        file: expected.file.clone(),
        language: expected.language.clone(),
        required_declaration_names: Vec::new(),
        forbidden_declaration_names: Vec::new(),
        expected_parse: expected.expected_parse.clone(),
        execution_status: "SOURCE_EXTRACTION_PASS".to_owned(),
    };
    validate_parse_expectation(&parse_expectation, result)?;
    if !result.capabilities.extractor_ready
        || !result.capabilities.declarations_ready
        || !result.capabilities.scopes_ready
        || !result.capabilities.imports_ready
        || !result.capabilities.references_ready
        || !result.capabilities.call_sites_ready
        || result.capabilities.name_resolution_ready
    {
        return Err(format!(
            "fixture {} has incorrect Rust/Go capability claims",
            expected.id
        ));
    }
    for declaration in &expected.required_declarations {
        let found = result.declarations.iter().any(|actual| {
            actual.kind == declaration.kind
                && actual.spelling == declaration.name
                && declaration
                    .container
                    .as_ref()
                    .is_none_or(|container| actual.container.as_ref() == Some(container))
                && declaration
                    .receiver_type
                    .as_ref()
                    .is_none_or(|receiver| actual.receiver_type.as_ref() == Some(receiver))
                && declaration
                    .signature_contains
                    .as_ref()
                    .is_none_or(|needle| {
                        actual
                            .signature
                            .as_ref()
                            .is_some_and(|signature| signature.contains(needle))
                    })
        });
        if !found {
            return Err(format!(
                "fixture {} is missing required declaration {}:{}",
                expected.id, declaration.kind, declaration.name
            ));
        }
    }
    for import in &expected.required_imports {
        if !result.imports.iter().any(|actual| {
            actual.spelling == import.path
                && actual.alias == import.alias
                && import
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| actual.limitations.contains(limitation))
        }) {
            return Err(format!(
                "fixture {} is missing required import {} alias {:?}",
                expected.id, import.path, import.alias
            ));
        }
    }
    for call in &expected.required_calls {
        if !result.call_sites.iter().any(|actual| {
            actual.kind == call.kind
                && actual.spelling == call.name
                && call
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| actual.attributes.contains(attribute))
        }) {
            return Err(format!(
                "fixture {} is missing required call {}:{}",
                expected.id, call.kind, call.name
            ));
        }
    }
    for condition in &expected.required_conditions {
        if !result
            .conditions
            .iter()
            .any(|actual| actual.kind == *condition)
        {
            return Err(format!(
                "fixture {} is missing required condition {condition}",
                expected.id
            ));
        }
    }
    let all_observations = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .chain(&result.imports)
        .chain(&result.conditions);
    for forbidden in &expected.forbidden_observations {
        if all_observations
            .clone()
            .any(|actual| actual.spelling == *forbidden)
        {
            return Err(format!(
                "fixture {} emitted forbidden observation {forbidden}",
                expected.id
            ));
        }
    }
    if result
        .call_sites
        .iter()
        .any(|call| call.target_id.is_some() || call.resolution != ResolutionCategory::Unresolved)
    {
        return Err(format!(
            "fixture {} claimed a resolved source-level call target",
            expected.id
        ));
    }
    if result.language == LanguageId::Rust
        && result.call_sites.iter().any(|call| {
            call.kind == "macro"
                && !call
                    .limitations
                    .iter()
                    .any(|limitation| limitation == "macro_not_expanded")
        })
    {
        return Err(format!(
            "fixture {} omitted the Rust macro-expansion limitation",
            expected.id
        ));
    }
    if result
        .conditions
        .iter()
        .any(|condition| condition.limitations.is_empty())
    {
        return Err(format!(
            "fixture {} emitted a condition without its configuration limitation",
            expected.id
        ));
    }
    if let Some(observation) = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .find(|observation| observation.scope_id.is_none())
    {
        return Err(format!(
            "fixture {} emitted an unscoped declaration/reference/call {}:{}",
            expected.id, observation.kind, observation.spelling
        ));
    }
    let mut ids = HashSet::new();
    for observation in result
        .declarations
        .iter()
        .chain(&result.scopes)
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
        .chain(&result.conditions)
    {
        if !ids.insert(observation.id.as_str()) {
            return Err(format!(
                "fixture {} emitted duplicate observation id {}",
                expected.id, observation.id
            ));
        }
    }
    for unicode_name in ["özet", "Özet"] {
        if let Some(observation) = result
            .declarations
            .iter()
            .find(|item| item.spelling == unicode_name)
        {
            let bytes = source
                .get(
                    observation.range.bytes.start() as usize
                        ..observation.range.bytes.end() as usize,
                )
                .ok_or_else(|| format!("fixture {} has invalid Unicode range", expected.id))?;
            if bytes != unicode_name.as_bytes() {
                return Err(format!(
                    "fixture {} did not preserve UTF-8 byte coordinates for {unicode_name}",
                    expected.id
                ));
            }
        }
    }
    Ok(())
}

fn append_rust_go_snapshot(
    snapshot: &mut String,
    expected: &RustGoExpectation,
    result: &ExtractionResult,
) {
    fn observations(items: &[SyntaxObservation]) -> String {
        let mut hasher = blake3::Hasher::new();
        for item in items {
            for value in [
                item.id.as_str(),
                item.kind.as_str(),
                item.spelling.as_str(),
                item.scope_id.as_deref().unwrap_or(""),
                item.container.as_deref().unwrap_or(""),
                item.signature.as_deref().unwrap_or(""),
                item.receiver_type.as_deref().unwrap_or(""),
                item.alias.as_deref().unwrap_or(""),
                item.target_id.as_deref().unwrap_or(""),
            ] {
                hasher.update(value.as_bytes());
                hasher.update(&[0]);
            }
            hasher.update(&item.range.bytes.start().to_le_bytes());
            hasher.update(&item.range.bytes.end().to_le_bytes());
            hasher.update(format!("{:?}", item.resolution).as_bytes());
            for attribute in &item.attributes {
                hasher.update(attribute.as_bytes());
                hasher.update(&[1]);
            }
            for limitation in &item.limitations {
                hasher.update(limitation.as_bytes());
                hasher.update(&[2]);
            }
        }
        format!("{}:{}", items.len(), hasher.finalize().to_hex())
    }
    snapshot.push_str(&format!(
        "id={} language={} parse={} source_hash={} query_hash={} extractor_hash={} declarations={} scopes={} imports={} references={} calls={} conditions={}\n",
        expected.id,
        result.language,
        if result.coverage.root_has_error {
            "partial"
        } else {
            "complete"
        },
        result.source_hash,
        result.query_fingerprint,
        result.extractor_fingerprint,
        observations(&result.declarations),
        observations(&result.scopes),
        observations(&result.imports),
        observations(&result.references),
        observations(&result.call_sites),
        observations(&result.conditions),
    ));
}

fn js_ts_fixtures() -> Result<(), i32> {
    let root = workspace_root();
    let actual = js_ts_snapshot(&root).map_err(|error| {
        eprintln!("js-ts-fixtures failed: {error}");
        6
    })?;
    let golden_path = root.join("fixtures/js-ts.golden");
    let golden = fs::read_to_string(&golden_path).map_err(|error| {
        eprintln!(
            "js-ts-fixtures failed: cannot read reviewed golden {}: {error}\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        6
    })?;
    if actual != golden {
        eprintln!(
            "js-ts-fixtures failed: reviewed golden {} differs\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        return Err(6);
    }
    print!("{actual}");
    Ok(())
}

fn js_ts_snapshot(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("fixtures/js-ts-expectations.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: JsTsManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported JavaScript/TypeScript fixture schema version {}",
            manifest.schema_version
        ));
    }
    let fixture_root = root
        .join("fixtures")
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize fixture root: {error}"))?;
    let mut worker = ParserWorker::new();
    let mut snapshot = String::new();
    for expected in &manifest.fixtures {
        let relative = Path::new(&expected.file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!("fixture {} has unsafe path", expected.id));
        }
        let canonical = root
            .join(relative)
            .canonicalize()
            .map_err(|error| format!("cannot open fixture {}: {error}", expected.id))?;
        if !canonical.starts_with(&fixture_root) {
            return Err(format!("fixture {} escapes fixtures root", expected.id));
        }
        let source = fs::read(&canonical)
            .map_err(|error| format!("cannot read fixture {}: {error}", expected.id))?;
        let language = expected
            .language
            .parse::<LanguageId>()
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if !matches!(
            language,
            LanguageId::JavaScript | LanguageId::Jsx | LanguageId::TypeScript | LanguageId::Tsx
        ) {
            return Err(format!(
                "fixture {} is not a JavaScript/TypeScript dialect",
                expected.id
            ));
        }
        let detected = LanguageRegistry::detect(relative)
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if detected.id != language {
            return Err(format!(
                "fixture {} labels {} but path detects {}",
                expected.id, language, detected.id
            ));
        }
        let result = worker
            .parse_file(
                language,
                relative,
                &source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        validate_js_ts_fixture(expected, &source, &result)?;
        append_js_ts_snapshot(&mut snapshot, expected, &result);
    }
    Ok(snapshot)
}

fn validate_js_ts_fixture(
    expected: &JsTsExpectation,
    source: &[u8],
    result: &ExtractionResult,
) -> Result<(), String> {
    let parse_expectation = FixtureExpectation {
        id: expected.id.clone(),
        file: expected.file.clone(),
        language: expected.language.clone(),
        required_declaration_names: Vec::new(),
        forbidden_declaration_names: Vec::new(),
        expected_parse: expected.expected_parse.clone(),
        execution_status: "SOURCE_EXTRACTION_PASS".to_owned(),
    };
    validate_parse_expectation(&parse_expectation, result)?;
    if !result.capabilities.extractor_ready
        || !result.capabilities.declarations_ready
        || !result.capabilities.scopes_ready
        || !result.capabilities.imports_ready
        || !result.capabilities.references_ready
        || !result.capabilities.call_sites_ready
        || result.capabilities.name_resolution_ready
    {
        return Err(format!(
            "fixture {} has incorrect JavaScript/TypeScript capability claims",
            expected.id
        ));
    }
    for declaration in &expected.required_declarations {
        if !result.declarations.iter().any(|actual| {
            actual.kind == declaration.kind
                && actual.spelling == declaration.name
                && declaration
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| actual.attributes.contains(attribute))
                && declaration
                    .signature_contains
                    .as_ref()
                    .is_none_or(|needle| {
                        actual
                            .signature
                            .as_ref()
                            .is_some_and(|signature| signature.contains(needle))
                    })
        }) {
            return Err(format!(
                "fixture {} is missing required declaration {}:{}",
                expected.id, declaration.kind, declaration.name
            ));
        }
    }
    for import in &expected.required_imports {
        if !result.imports.iter().any(|actual| {
            actual.kind == import.kind
                && actual.spelling == import.path
                && actual.alias == import.alias
                && import
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| actual.attributes.contains(attribute))
                && import
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| actual.limitations.contains(limitation))
        }) {
            return Err(format!(
                "fixture {} is missing required import/export {}:{} alias {:?}",
                expected.id, import.kind, import.path, import.alias
            ));
        }
    }
    validate_js_ts_occurrences(
        &expected.id,
        "call",
        &expected.required_calls,
        &result.call_sites,
    )?;
    validate_js_ts_occurrences(
        &expected.id,
        "reference",
        &expected.required_references,
        &result.references,
    )?;
    let all_observations = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .chain(&result.imports);
    for forbidden in &expected.forbidden_observations {
        if all_observations
            .clone()
            .any(|actual| actual.spelling == *forbidden)
        {
            return Err(format!(
                "fixture {} emitted forbidden observation {forbidden}",
                expected.id
            ));
        }
    }
    if result
        .call_sites
        .iter()
        .any(|call| call.target_id.is_some() || call.resolution != ResolutionCategory::Unresolved)
    {
        return Err(format!(
            "fixture {} claimed a resolved ECMAScript call target",
            expected.id
        ));
    }
    let jsx_references = result
        .references
        .iter()
        .filter(|reference| reference.kind == "jsx_component")
        .collect::<Vec<_>>();
    if jsx_references.iter().any(|reference| {
        reference
            .limitations
            .iter()
            .all(|limitation| limitation != "jsx_tag_not_a_call_edge")
            || result
                .call_sites
                .iter()
                .any(|call| call.range == reference.range)
    }) {
        return Err(format!(
            "fixture {} overclaimed JSX syntax as a call edge",
            expected.id
        ));
    }
    if let Some(observation) = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .find(|observation| observation.scope_id.is_none())
    {
        return Err(format!(
            "fixture {} emitted an unscoped declaration/reference/call {}:{}",
            expected.id, observation.kind, observation.spelling
        ));
    }
    if expected.id == "typescript-advanced" {
        let tokens = result
            .declarations
            .iter()
            .filter(|declaration| declaration.spelling == "Token")
            .collect::<Vec<_>>();
        if tokens.len() != 2
            || tokens[0].id == tokens[1].id
            || !tokens
                .iter()
                .any(|declaration| declaration.attributes.contains(&"role:type".to_owned()))
            || !tokens
                .iter()
                .any(|declaration| declaration.attributes.contains(&"role:value".to_owned()))
        {
            return Err(
                "typescript-advanced conflated same-name type/value declarations".to_owned(),
            );
        }
    }
    let mut ids = HashSet::new();
    for observation in result
        .declarations
        .iter()
        .chain(&result.scopes)
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
    {
        if !ids.insert(observation.id.as_str()) {
            return Err(format!(
                "fixture {} emitted duplicate observation id {}",
                expected.id, observation.id
            ));
        }
    }
    for observation in result
        .declarations
        .iter()
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
    {
        let bytes = source
            .get(observation.range.bytes.start() as usize..observation.range.bytes.end() as usize)
            .ok_or_else(|| format!("fixture {} has an invalid byte range", expected.id))?;
        if bytes != observation.spelling.as_bytes()
            && !matches!(
                observation.kind.as_str(),
                "esm_import" | "reexport" | "dynamic_import" | "commonjs_require"
            )
        {
            return Err(format!(
                "fixture {} range does not address spelling {:?} for kind {}",
                expected.id, observation.spelling, observation.kind
            ));
        }
    }
    Ok(())
}

fn validate_js_ts_occurrences(
    fixture_id: &str,
    category: &str,
    required: &[JsTsOccurrence],
    actual: &[SyntaxObservation],
) -> Result<(), String> {
    for expected in required {
        if !actual.iter().any(|item| {
            item.kind == expected.kind
                && item.spelling == expected.name
                && expected
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| item.attributes.contains(attribute))
                && expected
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| item.limitations.contains(limitation))
        }) {
            return Err(format!(
                "fixture {fixture_id} is missing required {category} {}:{}",
                expected.kind, expected.name
            ));
        }
    }
    Ok(())
}

fn append_js_ts_snapshot(
    snapshot: &mut String,
    expected: &JsTsExpectation,
    result: &ExtractionResult,
) {
    fn observations(items: &[SyntaxObservation]) -> String {
        let mut hasher = blake3::Hasher::new();
        for item in items {
            for value in [
                item.id.as_str(),
                item.kind.as_str(),
                item.spelling.as_str(),
                item.scope_id.as_deref().unwrap_or(""),
                item.container.as_deref().unwrap_or(""),
                item.signature.as_deref().unwrap_or(""),
                item.alias.as_deref().unwrap_or(""),
            ] {
                hasher.update(value.as_bytes());
                hasher.update(&[0]);
            }
            hasher.update(&item.range.bytes.start().to_le_bytes());
            hasher.update(&item.range.bytes.end().to_le_bytes());
            hasher.update(format!("{:?}", item.resolution).as_bytes());
            for attribute in &item.attributes {
                hasher.update(attribute.as_bytes());
                hasher.update(&[1]);
            }
            for limitation in &item.limitations {
                hasher.update(limitation.as_bytes());
                hasher.update(&[2]);
            }
        }
        format!("{}:{}", items.len(), hasher.finalize().to_hex())
    }
    snapshot.push_str(&format!(
        "id={} language={} parse={} source_hash={} query_hash={} extractor_hash={} declarations={} scopes={} imports={} references={} calls={}\n",
        expected.id,
        result.language,
        if result.coverage.root_has_error { "partial" } else { "complete" },
        result.source_hash,
        result.query_fingerprint,
        result.extractor_fingerprint,
        observations(&result.declarations),
        observations(&result.scopes),
        observations(&result.imports),
        observations(&result.references),
        observations(&result.call_sites),
    ));
}

fn csharp_java_fixtures() -> Result<(), i32> {
    let root = workspace_root();
    let actual = csharp_java_snapshot(&root).map_err(|error| {
        eprintln!("csharp-java-fixtures failed: {error}");
        7
    })?;
    let golden_path = root.join("fixtures/csharp-java.golden");
    let golden = fs::read_to_string(&golden_path).map_err(|error| {
        eprintln!(
            "csharp-java-fixtures failed: cannot read reviewed golden {}: {error}\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        7
    })?;
    if actual != golden {
        eprintln!(
            "csharp-java-fixtures failed: reviewed golden {} differs\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        return Err(7);
    }
    print!("{actual}");
    Ok(())
}

fn csharp_java_snapshot(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("fixtures/csharp-java-expectations.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: CSharpJavaManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported C#/Java fixture schema version {}",
            manifest.schema_version
        ));
    }
    let fixture_root = root
        .join("fixtures")
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize fixture root: {error}"))?;
    let mut worker = ParserWorker::new();
    let mut snapshot = String::new();
    let mut invoice_partial_ids = HashSet::new();
    for expected in &manifest.fixtures {
        let relative = Path::new(&expected.file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!("fixture {} has unsafe path", expected.id));
        }
        let canonical = root
            .join(relative)
            .canonicalize()
            .map_err(|error| format!("cannot open fixture {}: {error}", expected.id))?;
        if !canonical.starts_with(&fixture_root) {
            return Err(format!("fixture {} escapes fixtures root", expected.id));
        }
        let source = fs::read(&canonical)
            .map_err(|error| format!("cannot read fixture {}: {error}", expected.id))?;
        let language = expected
            .language
            .parse::<LanguageId>()
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if !matches!(language, LanguageId::CSharp | LanguageId::Java) {
            return Err(format!("fixture {} is not C# or Java", expected.id));
        }
        let detected = LanguageRegistry::detect(relative)
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if detected.id != language {
            return Err(format!(
                "fixture {} labels {} but path detects {}",
                expected.id, language, detected.id
            ));
        }
        let result = worker
            .parse_file(
                language,
                relative,
                &source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        validate_csharp_java_fixture(expected, &source, &result)?;
        for declaration in &result.declarations {
            if declaration.spelling == "Invoice"
                && declaration
                    .attributes
                    .contains(&"partial_group_hint:Billing.Invoice".to_owned())
            {
                invoice_partial_ids.insert(declaration.id.clone());
            }
        }
        append_csharp_java_snapshot(&mut snapshot, expected, &result);
    }
    if invoice_partial_ids.len() != 2 {
        return Err(format!(
            "expected two distinct Billing.Invoice partial declarations, found {}",
            invoice_partial_ids.len()
        ));
    }
    Ok(snapshot)
}

fn validate_csharp_java_fixture(
    expected: &CSharpJavaExpectation,
    source: &[u8],
    result: &ExtractionResult,
) -> Result<(), String> {
    let parse_expectation = FixtureExpectation {
        id: expected.id.clone(),
        file: expected.file.clone(),
        language: expected.language.clone(),
        required_declaration_names: Vec::new(),
        forbidden_declaration_names: Vec::new(),
        expected_parse: expected.expected_parse.clone(),
        execution_status: "SOURCE_EXTRACTION_PASS".to_owned(),
    };
    validate_parse_expectation(&parse_expectation, result)?;
    if !result.capabilities.extractor_ready
        || !result.capabilities.declarations_ready
        || !result.capabilities.scopes_ready
        || !result.capabilities.imports_ready
        || !result.capabilities.references_ready
        || !result.capabilities.call_sites_ready
        || result.capabilities.name_resolution_ready
    {
        return Err(format!(
            "fixture {} has incorrect C#/Java capability claims",
            expected.id
        ));
    }
    for declaration in &expected.required_declarations {
        if !result.declarations.iter().any(|actual| {
            actual.kind == declaration.kind
                && actual.spelling == declaration.name
                && declaration
                    .container
                    .as_ref()
                    .is_none_or(|container| actual.container.as_ref() == Some(container))
                && declaration
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| actual.attributes.contains(attribute))
                && declaration
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| actual.limitations.contains(limitation))
                && declaration
                    .signature_contains
                    .as_ref()
                    .is_none_or(|needle| {
                        actual
                            .signature
                            .as_ref()
                            .is_some_and(|signature| signature.contains(needle))
                    })
        }) {
            return Err(format!(
                "fixture {} is missing required declaration {}:{}",
                expected.id, declaration.kind, declaration.name
            ));
        }
    }
    for import in &expected.required_imports {
        if !result.imports.iter().any(|actual| {
            actual.kind == import.kind
                && actual.spelling == import.path
                && actual.alias == import.alias
                && import
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| actual.attributes.contains(attribute))
        }) {
            return Err(format!(
                "fixture {} is missing required import {}:{}",
                expected.id, import.kind, import.path
            ));
        }
    }
    validate_csharp_java_occurrences(
        &expected.id,
        "call",
        &expected.required_calls,
        &result.call_sites,
    )?;
    validate_csharp_java_occurrences(
        &expected.id,
        "reference",
        &expected.required_references,
        &result.references,
    )?;
    for condition in &expected.required_conditions {
        if !result
            .conditions
            .iter()
            .any(|actual| actual.spelling == *condition && !actual.limitations.is_empty())
        {
            return Err(format!(
                "fixture {} is missing required condition {condition}",
                expected.id
            ));
        }
    }
    let all_observations = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .chain(&result.imports)
        .chain(&result.conditions);
    for forbidden in &expected.forbidden_observations {
        if all_observations
            .clone()
            .any(|actual| actual.spelling == *forbidden)
        {
            return Err(format!(
                "fixture {} emitted forbidden observation {forbidden}",
                expected.id
            ));
        }
    }
    if result
        .call_sites
        .iter()
        .any(|call| call.target_id.is_some() || call.resolution != ResolutionCategory::Unresolved)
    {
        return Err(format!(
            "fixture {} claimed a resolved C#/Java call target",
            expected.id
        ));
    }
    if result.references.iter().any(|reference| {
        reference.kind == "method_reference"
            && (!reference
                .limitations
                .contains(&"method_reference_not_a_call_edge".to_owned())
                || result
                    .call_sites
                    .iter()
                    .any(|call| call.range == reference.range))
    }) {
        return Err(format!(
            "fixture {} overclaimed a Java method reference as a call",
            expected.id
        ));
    }
    if let Some(observation) = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .find(|observation| observation.scope_id.is_none())
    {
        return Err(format!(
            "fixture {} emitted an unscoped declaration/reference/call {}:{}",
            expected.id, observation.kind, observation.spelling
        ));
    }
    let mut ids = HashSet::new();
    for observation in result
        .declarations
        .iter()
        .chain(&result.scopes)
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
        .chain(&result.conditions)
    {
        if !ids.insert(observation.id.as_str()) {
            return Err(format!(
                "fixture {} emitted duplicate observation id {}",
                expected.id, observation.id
            ));
        }
    }
    for observation in result
        .declarations
        .iter()
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
    {
        let bytes = source
            .get(observation.range.bytes.start() as usize..observation.range.bytes.end() as usize)
            .ok_or_else(|| format!("fixture {} has an invalid byte range", expected.id))?;
        if bytes != observation.spelling.as_bytes() {
            return Err(format!(
                "fixture {} range does not address spelling {:?} for kind {}",
                expected.id, observation.spelling, observation.kind
            ));
        }
    }
    Ok(())
}

fn validate_csharp_java_occurrences(
    fixture_id: &str,
    category: &str,
    required: &[CSharpJavaOccurrence],
    actual: &[SyntaxObservation],
) -> Result<(), String> {
    for expected in required {
        if !actual.iter().any(|item| {
            item.kind == expected.kind
                && item.spelling == expected.name
                && expected
                    .receiver_expression
                    .as_ref()
                    .is_none_or(|receiver| item.receiver_type.as_ref() == Some(receiver))
                && expected
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| item.attributes.contains(attribute))
                && expected
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| item.limitations.contains(limitation))
        }) {
            return Err(format!(
                "fixture {fixture_id} is missing required {category} {}:{}",
                expected.kind, expected.name
            ));
        }
    }
    Ok(())
}

fn append_csharp_java_snapshot(
    snapshot: &mut String,
    expected: &CSharpJavaExpectation,
    result: &ExtractionResult,
) {
    fn observations(items: &[SyntaxObservation]) -> String {
        let mut hasher = blake3::Hasher::new();
        for item in items {
            for value in [
                item.id.as_str(),
                item.kind.as_str(),
                item.spelling.as_str(),
                item.scope_id.as_deref().unwrap_or(""),
                item.container.as_deref().unwrap_or(""),
                item.signature.as_deref().unwrap_or(""),
                item.receiver_type.as_deref().unwrap_or(""),
                item.alias.as_deref().unwrap_or(""),
                item.target_id.as_deref().unwrap_or(""),
            ] {
                hasher.update(value.as_bytes());
                hasher.update(&[0]);
            }
            hasher.update(&item.range.bytes.start().to_le_bytes());
            hasher.update(&item.range.bytes.end().to_le_bytes());
            hasher.update(format!("{:?}", item.resolution).as_bytes());
            for attribute in &item.attributes {
                hasher.update(attribute.as_bytes());
                hasher.update(&[1]);
            }
            for limitation in &item.limitations {
                hasher.update(limitation.as_bytes());
                hasher.update(&[2]);
            }
        }
        format!("{}:{}", items.len(), hasher.finalize().to_hex())
    }
    snapshot.push_str(&format!(
        "id={} language={} parse={} source_hash={} query_hash={} extractor_hash={} declarations={} scopes={} imports={} references={} calls={} conditions={}\n",
        expected.id,
        result.language,
        if result.coverage.root_has_error { "partial" } else { "complete" },
        result.source_hash,
        result.query_fingerprint,
        result.extractor_fingerprint,
        observations(&result.declarations),
        observations(&result.scopes),
        observations(&result.imports),
        observations(&result.references),
        observations(&result.call_sites),
        observations(&result.conditions),
    ));
}

fn dart_fixtures() -> Result<(), i32> {
    let root = workspace_root();
    let actual = dart_snapshot(&root).map_err(|error| {
        eprintln!("dart-fixtures failed: {error}");
        8
    })?;
    let golden_path = root.join("fixtures/dart.golden");
    let golden = fs::read_to_string(&golden_path).map_err(|error| {
        eprintln!(
            "dart-fixtures failed: cannot read reviewed golden {}: {error}\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        8
    })?;
    if actual != golden {
        eprintln!(
            "dart-fixtures failed: reviewed golden {} differs\n--- actual snapshot ---\n{actual}",
            golden_path.display()
        );
        return Err(8);
    }
    print!("{actual}");
    Ok(())
}

fn dart_snapshot(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("fixtures/dart-expectations.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: DartManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported Dart fixture schema version {}",
            manifest.schema_version
        ));
    }
    let fixture_root = root
        .join("fixtures")
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize fixture root: {error}"))?;
    let mut worker = ParserWorker::new();
    let mut snapshot = String::new();
    for expected in &manifest.fixtures {
        let relative = Path::new(&expected.file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!("fixture {} has unsafe path", expected.id));
        }
        let canonical = root
            .join(relative)
            .canonicalize()
            .map_err(|error| format!("cannot open fixture {}: {error}", expected.id))?;
        if !canonical.starts_with(&fixture_root) {
            return Err(format!("fixture {} escapes fixtures root", expected.id));
        }
        let source = fs::read(&canonical)
            .map_err(|error| format!("cannot read fixture {}: {error}", expected.id))?;
        let language = expected
            .language
            .parse::<LanguageId>()
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if language != LanguageId::Dart {
            return Err(format!("fixture {} is not Dart", expected.id));
        }
        let detected = LanguageRegistry::detect(relative)
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        if detected.id != language {
            return Err(format!(
                "fixture {} labels {} but path detects {}",
                expected.id, language, detected.id
            ));
        }
        let result = worker
            .parse_file(
                language,
                relative,
                &source,
                ParseLimits::default(),
                &CancellationContext::default(),
            )
            .map_err(|error| format!("fixture {}: {error}", expected.id))?;
        validate_dart_fixture(expected, &source, &result)?;
        append_dart_snapshot(&mut snapshot, expected, &result);
    }
    Ok(snapshot)
}

fn validate_dart_fixture(
    expected: &DartExpectation,
    source: &[u8],
    result: &ExtractionResult,
) -> Result<(), String> {
    let parse_expectation = FixtureExpectation {
        id: expected.id.clone(),
        file: expected.file.clone(),
        language: expected.language.clone(),
        required_declaration_names: Vec::new(),
        forbidden_declaration_names: Vec::new(),
        expected_parse: expected.expected_parse.clone(),
        execution_status: "SOURCE_EXTRACTION_PASS".to_owned(),
    };
    validate_parse_expectation(&parse_expectation, result)?;
    if !result.capabilities.extractor_ready
        || !result.capabilities.declarations_ready
        || !result.capabilities.scopes_ready
        || !result.capabilities.imports_ready
        || !result.capabilities.references_ready
        || !result.capabilities.call_sites_ready
        || result.capabilities.name_resolution_ready
    {
        return Err(format!(
            "fixture {} has incorrect Dart capability claims",
            expected.id
        ));
    }
    for declaration in &expected.required_declarations {
        if !result.declarations.iter().any(|actual| {
            actual.kind == declaration.kind
                && actual.spelling == declaration.name
                && declaration
                    .container
                    .as_ref()
                    .is_none_or(|container| actual.container.as_ref() == Some(container))
                && declaration
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| actual.attributes.contains(attribute))
                && declaration
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| actual.limitations.contains(limitation))
                && declaration
                    .signature_contains
                    .as_ref()
                    .is_none_or(|needle| {
                        actual
                            .signature
                            .as_ref()
                            .is_some_and(|signature| signature.contains(needle))
                    })
        }) {
            return Err(format!(
                "fixture {} is missing required declaration {}:{}",
                expected.id, declaration.kind, declaration.name
            ));
        }
    }
    for import in &expected.required_imports {
        if !result.imports.iter().any(|actual| {
            actual.kind == import.kind
                && actual.spelling == import.path
                && actual.alias == import.alias
                && import
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| actual.attributes.contains(attribute))
                && import
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| actual.limitations.contains(limitation))
        }) {
            return Err(format!(
                "fixture {} is missing required Dart library relation {}:{}",
                expected.id, import.kind, import.path
            ));
        }
    }
    validate_dart_occurrences(
        &expected.id,
        "call",
        &expected.required_calls,
        &result.call_sites,
    )?;
    validate_dart_occurrences(
        &expected.id,
        "reference",
        &expected.required_references,
        &result.references,
    )?;
    for fragment in &expected.required_diagnostic_fragments {
        if !result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.contains(fragment) || diagnostic.message.contains(fragment)
        }) {
            return Err(format!(
                "fixture {} is missing diagnostic fragment {fragment}",
                expected.id
            ));
        }
    }
    let all_observations = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .chain(&result.imports);
    for forbidden in &expected.forbidden_observations {
        if all_observations
            .clone()
            .any(|actual| actual.spelling == *forbidden)
        {
            return Err(format!(
                "fixture {} emitted forbidden observation {forbidden}",
                expected.id
            ));
        }
    }
    if result
        .call_sites
        .iter()
        .any(|call| call.target_id.is_some() || call.resolution != ResolutionCategory::Unresolved)
    {
        return Err(format!(
            "fixture {} claimed a resolved Dart call target",
            expected.id
        ));
    }
    if result
        .call_sites
        .iter()
        .filter(|call| call.kind == "constructor_like")
        .any(|call| {
            !call
                .limitations
                .contains(&"constructor_or_function_target_not_resolved".to_owned())
                || !call
                    .limitations
                    .contains(&"flutter_widget_identity_and_tree_not_inferred".to_owned())
        })
    {
        return Err(format!(
            "fixture {} overclaimed capitalized Dart call syntax",
            expected.id
        ));
    }
    if result
        .declarations
        .iter()
        .filter(|declaration| declaration.kind.contains("constructor"))
        .any(|declaration| {
            !declaration
                .limitations
                .contains(&"constructor_identity_is_syntax_only".to_owned())
        })
    {
        return Err(format!(
            "fixture {} omitted the Dart constructor identity limitation",
            expected.id
        ));
    }
    if let Some(observation) = result
        .declarations
        .iter()
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
        .find(|observation| observation.scope_id.is_none())
    {
        return Err(format!(
            "fixture {} emitted an unscoped observation {}:{}",
            expected.id, observation.kind, observation.spelling
        ));
    }
    let mut ids = HashSet::new();
    for observation in result
        .declarations
        .iter()
        .chain(&result.scopes)
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
    {
        if !ids.insert(observation.id.as_str()) {
            return Err(format!(
                "fixture {} emitted duplicate observation id {}",
                expected.id, observation.id
            ));
        }
    }
    for observation in result
        .declarations
        .iter()
        .chain(&result.imports)
        .chain(&result.references)
        .chain(&result.call_sites)
    {
        let bytes = source
            .get(observation.range.bytes.start() as usize..observation.range.bytes.end() as usize)
            .ok_or_else(|| format!("fixture {} has an invalid byte range", expected.id))?;
        if bytes != observation.spelling.as_bytes() {
            return Err(format!(
                "fixture {} range does not address spelling {:?} for kind {}",
                expected.id, observation.spelling, observation.kind
            ));
        }
    }
    Ok(())
}

fn validate_dart_occurrences(
    fixture_id: &str,
    category: &str,
    required: &[DartOccurrence],
    actual: &[SyntaxObservation],
) -> Result<(), String> {
    for expected in required {
        if !actual.iter().any(|item| {
            item.kind == expected.kind
                && item.spelling == expected.name
                && expected
                    .receiver_expression
                    .as_ref()
                    .is_none_or(|receiver| item.receiver_type.as_ref() == Some(receiver))
                && expected
                    .attribute
                    .as_ref()
                    .is_none_or(|attribute| item.attributes.contains(attribute))
                && expected
                    .limitation
                    .as_ref()
                    .is_none_or(|limitation| item.limitations.contains(limitation))
        }) {
            return Err(format!(
                "fixture {fixture_id} is missing required {category} {}:{}",
                expected.kind, expected.name
            ));
        }
    }
    Ok(())
}

fn append_dart_snapshot(
    snapshot: &mut String,
    expected: &DartExpectation,
    result: &ExtractionResult,
) {
    fn observations(items: &[SyntaxObservation]) -> String {
        let mut hasher = blake3::Hasher::new();
        for item in items {
            for value in [
                item.id.as_str(),
                item.kind.as_str(),
                item.spelling.as_str(),
                item.scope_id.as_deref().unwrap_or(""),
                item.container.as_deref().unwrap_or(""),
                item.signature.as_deref().unwrap_or(""),
                item.receiver_type.as_deref().unwrap_or(""),
                item.alias.as_deref().unwrap_or(""),
                item.target_id.as_deref().unwrap_or(""),
            ] {
                hasher.update(value.as_bytes());
                hasher.update(&[0]);
            }
            hasher.update(&item.range.bytes.start().to_le_bytes());
            hasher.update(&item.range.bytes.end().to_le_bytes());
            hasher.update(format!("{:?}", item.resolution).as_bytes());
            for attribute in &item.attributes {
                hasher.update(attribute.as_bytes());
                hasher.update(&[1]);
            }
            for limitation in &item.limitations {
                hasher.update(limitation.as_bytes());
                hasher.update(&[2]);
            }
        }
        format!("{}:{}", items.len(), hasher.finalize().to_hex())
    }
    snapshot.push_str(&format!(
        "id={} language={} parse={} source_hash={} query_hash={} extractor_hash={} declarations={} scopes={} imports={} references={} calls={} conditions={}\n",
        expected.id,
        result.language,
        if result.coverage.root_has_error { "partial" } else { "complete" },
        result.source_hash,
        result.query_fingerprint,
        result.extractor_fingerprint,
        observations(&result.declarations),
        observations(&result.scopes),
        observations(&result.imports),
        observations(&result.references),
        observations(&result.call_sites),
        observations(&result.conditions),
    ));
}

fn validate_parse_expectation(
    expected: &FixtureExpectation,
    result: &ExtractionResult,
) -> Result<(), String> {
    match expected.expected_parse.as_str() {
        "complete" | "capability_probe" if result.coverage.root_has_error => Err(format!(
            "fixture {} expected a complete parse but has ERROR/MISSING syntax",
            expected.id
        )),
        "partial_with_diagnostic" if !result.coverage.root_has_error => Err(format!(
            "fixture {} expected an incomplete parse diagnostic",
            expected.id
        )),
        "complete" | "capability_probe" | "partial_with_diagnostic" => Ok(()),
        value => Err(format!(
            "fixture {} has unknown expected_parse {value}",
            expected.id
        )),
    }
}

fn validate_names(expected: &FixtureExpectation, result: &ExtractionResult) -> Result<(), String> {
    let declarations = result
        .declarations
        .iter()
        .map(|item| item.spelling.as_str())
        .collect::<Vec<_>>();
    let all_observations = result
        .declarations
        .iter()
        .chain(&result.references)
        .chain(&result.call_sites)
        .map(|item| item.spelling.as_str())
        .collect::<Vec<_>>();
    validate_name_sets(
        &expected.id,
        &expected.required_declaration_names,
        &expected.forbidden_declaration_names,
        &declarations,
        &all_observations,
    )
}

fn validate_name_sets(
    fixture_id: &str,
    required: &[String],
    forbidden: &[String],
    declarations: &[&str],
    all_observations: &[&str],
) -> Result<(), String> {
    for name in required {
        if !declarations.contains(&name.as_str()) {
            return Err(format!(
                "fixture {fixture_id} is missing required declaration {name}"
            ));
        }
    }
    for name in forbidden {
        if all_observations.contains(&name.as_str()) {
            return Err(format!(
                "fixture {fixture_id} emitted forbidden declaration/reference/call {name}"
            ));
        }
    }
    Ok(())
}

fn append_snapshot_line(
    snapshot: &mut String,
    expected: &FixtureExpectation,
    result: &ExtractionResult,
) {
    let declarations = result
        .declarations
        .iter()
        .map(|item| {
            format!(
                "{}:{}@{}-{}",
                item.kind,
                item.spelling,
                item.range.bytes.start(),
                item.range.bytes.end()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    snapshot.push_str(&format!(
        "id={} language={} parse={} source_hash={} grammar_hash={} query_hash={} declarations={}\n",
        expected.id,
        result.language,
        if result.coverage.root_has_error {
            "partial"
        } else {
            "complete"
        },
        result.source_hash,
        result.grammar_fingerprint,
        result.query_fingerprint,
        declarations
    ));
}

fn usage() {
    eprintln!(
        "usage: cargo run -p xtask -- <verify [--inject-failure] | grammar-check | fixtures | rust-go-fixtures | js-ts-fixtures | csharp-java-fixtures | dart-fixtures | phase14-real-corpus>"
    );
}

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let result = match arguments.as_slice() {
        [command] if command == "verify" => verify(false),
        [command, flag] if command == "verify" && flag == "--inject-failure" => verify(true),
        [command] if command == "grammar-check" => grammar_check(),
        [command] if command == "fixtures" => fixtures(),
        [command] if command == "rust-go-fixtures" => rust_go_fixtures(),
        [command] if command == "js-ts-fixtures" => js_ts_fixtures(),
        [command] if command == "csharp-java-fixtures" => csharp_java_fixtures(),
        [command] if command == "dart-fixtures" => dart_fixtures(),
        [command] if command == "phase14-real-corpus" => phase14_real_corpus(),
        _ => {
            usage();
            Err(2)
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_contains_the_required_workspace_gates() {
        assert_eq!(VERIFY_GATES.len(), 3);
        assert_eq!(VERIFY_GATES[0].arguments, ["fmt", "--all", "--", "--check"]);
        assert!(VERIFY_GATES[1].arguments.contains(&"clippy"));
        assert!(VERIFY_GATES[2].arguments.contains(&"--locked"));
    }

    #[test]
    fn injected_check_targets_a_nonexistent_feature() {
        assert!(
            DELIBERATE_FAILURE
                .arguments
                .contains(&"__codeatlas_deliberate_failure__")
        );
    }

    #[test]
    fn fixture_validator_rejects_missing_declarations_and_false_positive_references() {
        let required = vec!["required".to_owned()];
        let forbidden = vec!["phantom_call".to_owned()];
        let missing = validate_name_sets("missing", &required, &forbidden, &[], &[])
            .expect_err("missing declaration must fail");
        assert!(missing.contains("missing required declaration"));

        let false_positive =
            validate_name_sets("false-positive", &[], &forbidden, &[], &["phantom_call"])
                .expect_err("forbidden reference must fail");
        assert!(false_positive.contains("forbidden declaration/reference/call"));
    }
}
