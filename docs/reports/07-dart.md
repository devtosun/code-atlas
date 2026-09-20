# Phase 07 report — Dart extraction

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git worktree (`git status` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 / `aarch64-apple-darwin`; Rust 1.98.1 (`48a229ceaefd4985c50990b14116b6d856af0985c`, LLVM 22.1.8)

## Scope implemented

- Rechecked the pinned `nielsenko/tree-sitter-dart` 0.2.0 package, MIT license,
  ABI 15 binding, generated node types and representative S-expressions. No
  grammar package, ABI, checksum or native source changed.
- Replaced the parser-only Dart placeholder with separately embedded and compiled
  symbols/scopes, imports, references and calls queries plus a Dart source adapter.
- Added declarations and lexical scopes for libraries, classes, enums, mixins,
  extensions, extension types, functions/methods, getters/setters, fields/locals,
  parameters, type parameters, pattern bindings and closure/block scopes.
- Preserved ordinary, named, const, factory and redirecting constructor kinds with
  owner, signature, structural path and distinct path-sensitive IDs.
- Added library/import/export/part/part-of observations with exact URI/name ranges,
  aliases and unresolved package/part limitations. Import prefixes remain source
  evidence rather than bindings.
- Added unresolved call observations with receiver expression source, await and
  named-argument evidence. Capitalized invocations are only `constructor_like`;
  nested Flutter-style invocations are not promoted to a semantic widget tree.
- Added records, patterns, sealed classes, extension types and switch expressions
  as reviewed modern-syntax coverage. The pinned grammar's `ERROR` result for
  non-ASCII identifiers is explicitly diagnosed rather than reported as support.
- No Dart/Flutter SDK, analyzer, pub, build_runner, package configuration,
  generated code, dependency restore or indexed source is executed.
- Added eight focused cases with independent expectations and reviewed exact
  category hashes. Every earlier parser and focused language suite remains a gate.

## Files changed

- Extraction runtime: `crates/ca-languages/src/{adapters,lib,registry,worker}.rs`
- Queries: `crates/ca-languages/queries/dart/{symbols,imports,references,calls}.scm`
- Focused corpus: `fixtures/dart/{advanced,advanced_part,Broken,Unicode}.dart`
  plus the existing basic, modern and Flutter-style Dart fixtures
- Expectations/goldens: `fixtures/{dart-expectations.json,dart.golden,parser-kernel.golden}`
- Harness: `xtask/src/main.rs`
- State/decisions: `config/dependency-lock.json`,
  `docs/adr/0006-dart-syntax-extraction.md`,
  `docs/{ARCHITECTURE,DATA_MODEL,GRAMMAR_MATRIX,LANGUAGE_SUPPORT,PROJECT_STATE,TEST_MATRIX}.md`,
  `fixtures/README.md`

## Decisions and evidence

ADR-0006 records the syntax-only boundary, constructor/accessor identity,
unresolved package/prefix/part/call relationships, Flutter-style evidence and the
prohibition on SDK/analyzer/package/build execution.

The Dart query fingerprint is
`35d474ab9bf441c95cbd92962b462832310f90f8f09e9c81fbded72854f547cd` and
`dart-source-v1` produces extractor fingerprint
`db8d093a22450cffb5032a9472aaf795c0f4d0aaf04f9baef3f29e745bd405d1`.
The grammar fingerprint remains
`8191ecba8dadb79b8964a89b933a791f9cd145a7ea69067b5821ab3b4e96cf7c`.

Independent expectations assert declaration kinds, containers, signatures,
constructor owners, import relationships and aliases, unresolved calls, named
arguments, constructor-like/Flutter limitations, diagnostics and forbidden
comment/string names before exact snapshot comparison. The extractor output is not
used as its own semantic ground truth.

## Commands and evidence

All successful Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME`
and explicit toolchain `PATH` because no system Cargo is on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| `cargo run -p xtask --locked -- verify` (Phase 06 prerequisite, before edits) | 0 | 58 tests, 33 query assets and every prior fixture suite clean |
| `cargo run -p xtask --locked -- grammar-check` (first Dart query draft) | 3 | an invalid constant-constructor capture shape was rejected; corrected against the inspected grammar nodes |
| `cargo test -p ca-languages --locked` | 0 | 24/24 language tests pass, including four Dart adapter/worker tests |
| `cargo run -p xtask --locked -- dart-fixtures` (before golden creation) | 8 | semantic checks passed; missing reviewed golden reported, then added deliberately |
| `cargo run -p xtask --locked -- fixtures` (first parser-golden run) | 4 | expected Dart query/declaration fingerprint differences were reviewed and updated; non-Dart lines were preserved |
| `cargo run -p xtask --locked -- js-ts-fixtures` (regression diagnosis) | 6 | shared import-range filtering incorrectly treated a local JS `export` like a Dart export and hid `Panel.Item`; filtering was made language-specific |
| `cargo run -p xtask --locked -- grammar-check` | 0 | all 36 query assets compile against the nine pinned provider targets |
| `cargo run -p xtask --locked -- fixtures` | 0 | 24/24 parser fixtures match the reviewed golden |
| `cargo run -p xtask --locked -- rust-go-fixtures` | 0 | 8/8 Rust/Go regression cases unchanged |
| `cargo run -p xtask --locked -- js-ts-fixtures` | 0 | 11/11 JS/TS regression cases unchanged after the scoped fix |
| `cargo run -p xtask --locked -- csharp-java-fixtures` | 0 | 11/11 C#/Java regression cases unchanged |
| `cargo run -p xtask --locked -- dart-fixtures` | 0 | 8/8 Dart expectation and exact-hash lines pass |
| `cargo clippy --workspace --all-targets -- -D warnings` (first run) | 101 | two iterator-style diagnostics in the Dart adapter; replaced with `find`/`rfind` |
| `cargo run -p xtask --locked -- verify` (first final run) | 5 | a too-narrow follow-up filter exposed Go import identifiers; restored the proven shared exclusions and limited the new kinds to Dart |
| `cargo fmt --all -- --check` | 0 | formatting clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | no warnings |
| `cargo test --workspace --locked` | 0 | 62 passed; one storage helper ignored and exercised by its parent test |
| `jq empty config/dependency-lock.json fixtures/dart-expectations.json` | 0 | edited JSON documents valid |
| `cargo run -p xtask --locked -- verify` (final) | 0 | format, clippy, 62-test workspace, 36 query assets and all five fixture suites pass |

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Dart has declarations, scopes, library relationships, references and calls | passed | four compiled queries, adapter tests and focused fixture categories |
| Basic and supported modern Dart syntax parses and extracts | passed | original fixtures plus records, patterns, sealed classes, extension types and switch expressions |
| Unsupported modern forms are explicit | passed | non-ASCII identifiers produce a diagnostic and named grammar limitation |
| Flutter-style fixtures require no runtime | passed | in-process Rust parser only; nested calls, named arguments and callbacks asserted as syntax |
| Prefixes, accessors and constructor identities are not conflated | passed | alias/getter/setter/kind/owner/signature and distinct-ID assertions |
| Calls and external relationships retain uncertainty | passed | every call unresolved; package/part/prefix and constructor-like limitations asserted |
| Partial, negative, CRLF/UTF-8 and cancellation/reset cases pass | passed | malformed, forbidden-name, exact-range and worker reset tests |
| All required languages plus JSX/TSX retain their gates | passed | 24 parser, 8 Rust/Go, 11 JS/TS, 11 C#/Java and 8 Dart cases |
| Linux x64 native extraction | not run | only `aarch64-apple-darwin` is installed locally |
| Windows x64 MSVC native extraction | not run | native runner unavailable locally |

## Known limitations and risks

- This is syntax extraction, not analyzer resolution, type checking, package graph
  evaluation, part merging, receiver typing, overload/constructor selection,
  framework interpretation or runtime dispatch.
- The pinned grammar rejects non-ASCII Dart identifiers. Multibyte UTF-8 in
  comments/strings and exact CRLF byte ranges pass, but Unicode identifier support
  must not be inferred.
- Capitalization and optional `new` are insufficient to prove a constructor. The
  `constructor_like` attribute is evidence only and all targets remain unresolved.
- Flutter-style nesting, named arguments and callbacks do not establish widget
  identity, lifecycle, navigation or runtime parent/child relationships.
- Recovered facts under malformed `ERROR` trees may be incomplete. Diagnostics and
  partial parse coverage remain visible.
- The focused corpus establishes deterministic contract coverage, not real-world
  precision/recall. Parser fuzzing and native Linux/Windows execution remain pending.
- Parser work remains synchronous at this crate boundary. Phase 08 must place it
  behind bounded indexing workers before protocol use.
- No MCP extraction/search capability is exposed and observations are not persisted.

## Blockers and safe next actions

No Phase 07 blocker on the available macOS arm64 host. Do not infer cross-platform
execution, Unicode Dart identifiers, analyzer identities, package/part bindings,
semantic constructor targets or a Flutter widget tree from this result. Phase 08
can add bounded indexing jobs and atomic persistence using these syntax observations.

## Exact next prompt

`prompts/08-index-jobs.md`
