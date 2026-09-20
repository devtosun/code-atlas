# Phase 06 report — C# and Java extraction

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git worktree (`git status` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 / `aarch64-apple-darwin`; Rust 1.98.1 (`48a229ceaefd4985c50990b14116b6d856af0985c`, LLVM 22.1.8)

## Scope implemented

- Added separately embedded and compiled symbols/scopes, imports, references and
  calls queries for the pinned C# and Java grammars after inspecting their exact
  `node-types.json` metadata and representative parse trees.
- Added C# extraction for block/file-scoped namespaces, classes, structs,
  interfaces, enums, records, distinct partial declarations, fields, properties,
  accessors, constructors, generic/overloaded/async/local/extension methods,
  attributes, top-level declarations/calls and preprocessor conditions.
- Added Java extraction for packages, regular/static imports, annotation types,
  classes/interfaces/enums/records, nested types, fields, constructors,
  generic/overloaded methods, annotations, lambdas and method references.
- C# partial declarations remain separate path-sensitive records. A common
  namespace/container/name grouping hint is explicitly unconfirmed. Callable
  signatures and structural paths preserve overload and nested declaration identity.
- Member-call receiver expression source is retained without claiming a receiver
  type. Interface, virtual and extension dispatch remain unresolved with no target
  ID. Java method references remain reference observations, never call edges.
- Attribute/annotation arguments, DI-like strings and preprocessor branches are not
  evaluated. Malformed-tree diagnostics state that recovered `ERROR` subtrees are
  not fully understood.
- No .NET/JVM/compiler, MSBuild, Gradle/Maven, dependency restore, project
  evaluation, annotation processor, language server or indexed code is executed.
- Added eleven focused cases with independent expectations and reviewed exact
  category hashes. All earlier parser, Rust/Go and JS/TS fixture suites remain gates.

## Files changed

- Extraction runtime: `crates/ca-languages/src/{adapters,registry,worker}.rs`
- Queries: `crates/ca-languages/queries/{csharp,java}/{symbols,imports,references,calls}.scm`
- Focused corpus: advanced, top-level, malformed and Unicode/CRLF C#/Java files
  under `fixtures/{csharp,java}/`
- Expectations/goldens: `fixtures/{csharp-java-expectations.json,csharp-java.golden,parser-kernel.golden}`
- Harness: `xtask/src/main.rs`
- State/decisions: `config/dependency-lock.json`,
  `docs/adr/0005-csharp-java-syntax-extraction.md`,
  `docs/{ARCHITECTURE,DATA_MODEL,GRAMMAR_MATRIX,LANGUAGE_SUPPORT,PROJECT_STATE,TEST_MATRIX}.md`,
  `fixtures/README.md`

## Decisions and evidence

ADR-0005 records the syntax-only boundary, distinct partial declarations,
signature/structural overload identity, receiver-expression evidence and the
prohibition on compiler/build/project execution.

The C# query fingerprint is
`b20cc363dd724f72ca29d2fc9ad524ac78faf80adf0cb84332d475153a47b7b7` and
`csharp-source-v1` produces extractor fingerprint
`63965558f4a9f08211a54bb9fd1e4f1ab9f10cd8a7d6d26df2ec456846f5af79`.
The Java query fingerprint is
`04b31766ff2cbba3a51b4a4a956e9b5a47164e3cce03fcd824740f2f2ad58889`
and `java-source-v1` produces extractor fingerprint
`9207a9c2e38d4b5214e61a86d4c5d539b6136d9b86b8089bd4ccb2b02bf5dbe6`.

Independent expectations assert required declaration kinds, containers,
signatures, partial hints, import forms, receiver expressions, unresolved calls,
annotation/attribute and method-reference limitations, preprocessor conditions and
forbidden comment/string names before exact snapshot comparison. The extractor
output is not used as its own semantic ground truth.

## Commands and evidence

All successful Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME`
and explicit toolchain `PATH` because no system Cargo is on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| `cargo run -p xtask --locked -- verify` (Phase 05 prerequisite, before edits) | 0 | 54 tests, 27 query assets and every prior fixture suite clean |
| `cargo run -p xtask --locked -- grammar-check` (initial query compile) | 0 | all new C#/Java query assets compiled against pinned grammars |
| `cargo run -p xtask --locked -- csharp-java-fixtures` (before golden creation) | 7 | semantic checks passed after correcting hand labels; missing reviewed golden reported, then added deliberately |
| `cargo run -p xtask --locked -- rust-go-fixtures` (intermediate regression) | 5 | an over-broad reference exclusion changed prior hashes; the behavior was scoped to C#/Java and prior goldens were preserved |
| `cargo test -p ca-languages --locked` | 0 | 20/20 language tests pass |
| `cargo run -p xtask --locked -- fixtures` | 0 | 24/24 parser fixtures match the reviewed golden |
| `cargo run -p xtask --locked -- rust-go-fixtures` | 0 | 8/8 Rust/Go regression cases unchanged |
| `cargo run -p xtask --locked -- js-ts-fixtures` | 0 | 11/11 JS/TS regression cases unchanged |
| `cargo run -p xtask --locked -- csharp-java-fixtures` | 0 | 11/11 C#/Java expectation and exact-hash lines pass |
| `cargo clippy --workspace --all-targets -- -D warnings` (first run) | 101 | one `unnecessary_first_then_check` diagnostic; simplified the check |
| `cargo clippy --workspace --all-targets -- -D warnings` (final focused run) | 0 | no warnings |
| `cargo test --workspace --locked` | 0 | 58 passed; one storage helper ignored and exercised by its parent test |
| `cargo run -p xtask --locked -- verify` (first final run) | 1 | formatting diff only; `cargo fmt --all` applied before the clean rerun |
| `cargo fmt --all -- --check` | 0 | formatting clean |
| `jq empty config/dependency-lock.json fixtures/csharp-java-expectations.json` | 0 | edited JSON documents valid |
| `cargo run -p xtask --locked -- verify` (final) | 0 | format, clippy, 58-test workspace, 33 query assets and all four fixture suites pass |

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| C#/Java required constructs have distinct IDs and byte ranges | passed | focused expectations, range validator and reviewed golden |
| C# partial declarations remain separate with evidence-only grouping | passed | two `Billing.Invoice` paths have distinct IDs and a shared limited hint |
| Overloaded and nested declarations are not name-collapsed | passed | C# `Total`/`Echo`, Java `total`/`map`, nested container assertions |
| Interface/virtual/extension/DI uncertainty is retained | passed | every call unresolved; receiver source and reason-bearing limitations asserted; string negatives |
| Java annotations/lambdas/method references remain syntax evidence | passed | annotation and lambda fixtures; method reference explicitly not a call edge |
| C# preprocessor branches do not imply an active configuration | passed | `DEBUG` condition plus warning/limitation assertion |
| Syntax errors, comments/strings and Unicode CRLF negatives pass | passed | malformed C#/Java, forbidden names and exact byte-range checks |
| No .NET/JVM/build/restore/project execution | passed | in-process Rust parser/harness only; no runtime or subprocess path added |
| Previous parser and language suites remain green | passed | 24 parser, 8 Rust/Go and 11 JS/TS cases |
| Linux x64 native extraction | not run | only `aarch64-apple-darwin` is installed locally |
| Windows x64 MSVC native extraction | not run | native runner unavailable locally |

## Known limitations and risks

- This is syntax extraction, not C#/Java compilation, type checking, overload
  selection, inheritance analysis, classpath/project evaluation or runtime dispatch.
- Partial grouping hints can collide when compiler options, aliases or generated
  sources differ; no merged declaration is created.
- Receiver expression source is stored in the existing receiver field but is
  explicitly not an inferred receiver type.
- Both branches under C# preprocessor syntax may produce observations. The active
  symbol configuration is unknown.
- Attributes/annotations and their arguments are retained as syntax; processors,
  reflection behavior, framework registration and DI wiring are not inferred.
- Recovered facts under malformed `ERROR` trees may be incomplete. Diagnostics and
  partial parse coverage remain visible.
- The focused corpus establishes deterministic contract coverage, not real-world
  precision/recall. Parser fuzzing and native Linux/Windows execution remain pending.
- Parser work remains synchronous at this crate boundary. The indexing phase must
  place workers behind a bounded CPU queue before protocol use.
- No MCP extraction/search capability is exposed and observations are not persisted.

## Blockers and safe next actions

No Phase 06 blocker on the available macOS arm64 host. Do not infer cross-platform
execution, compiler identities, overload selection, active preprocessor branches,
classpath/project resolution or runtime dispatch from this result. Phase 07 can add
the Dart adapter under the same syntax-evidence and explicit-uncertainty boundary.

## Exact next prompt

`prompts/07-dart.md`
