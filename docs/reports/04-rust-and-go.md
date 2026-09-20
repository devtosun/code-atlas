# Phase 04 report — Rust and Go source extraction

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git worktree (`git status` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 / `aarch64-apple-darwin`; Rust 1.98.1 (`48a229ceaefd4985c50990b14116b6d856af0985c`, LLVM 22.1.8)

## Scope implemented

- Added separately embedded and compiled Rust/Go symbols, imports, references and
  calls queries, using the pinned grammar node types and inspected parse trees.
- Added language-specific adapters for complete Phase 04 syntax observations:
  declarations, lexical scopes, imports/packages, identifier-like references,
  call/macro sites and cfg/build-tag conditions.
- Rust extraction covers modules, grouped and aliased use trees, structs, enums,
  traits, inherent/trait impls, associated methods, generics, async functions,
  closures, macros, constants/statics and cfg/cfg_attr syntax.
- Go extraction covers package/import aliases, generic types, structs/interfaces,
  embedded fields, receiver methods, functions, closures, constructor-like call
  syntax and build tags. Method receiver type source and function signatures are
  retained.
- Extended owned observations with stable ID, containing syntax range, scope ID,
  container, signature, receiver type, alias, optional target, resolution category,
  attributes and limitations. Added condition observations and extractor fingerprints.
- Structural IDs use language, repository-relative path, category, kind, spelling
  and named-node structural path. Focused tests prove stability under line insertion
  and distinction for receiver methods and nested shadowing.
- All calls are explicitly `unresolved` with no target ID. Rust macros are not
  expanded; cfg and Go build constraints are recorded without inferring an active
  configuration. Matching trait/interface/method names are not treated as targets.
- Rust/Go capability flags now advertise parser, extractor and extraction-category
  readiness while keeping name-resolution readiness false. Other providers preserve
  Phase 03 parser-kernel behavior.
- Added eight focused Rust/Go cases plus independent expectations and a reviewed
  exact category-hash golden. The original 24-file parser golden remains a regression
  gate and was deliberately updated only for expanded Rust/Go output/query hashes.

## Files changed

- Extraction model/runtime: `crates/ca-languages/src/{adapters,lib,model,registry,worker}.rs`
- Rust/Go queries: `crates/ca-languages/queries/{rust,go}/{symbols,imports,references,calls}.scm`
- Focused corpus: `fixtures/{rust,go}/{domain,receivers,broken}.{rs,go}`,
  `fixtures/rust-go-expectations.json`, `fixtures/rust-go.golden`
- Regression harness: `fixtures/parser-kernel.golden`, `xtask/{Cargo.toml,src/main.rs}`
- State/decisions: `config/dependency-lock.json`,
  `docs/adr/0003-rust-go-source-extraction.md`,
  `docs/{ARCHITECTURE,DATA_MODEL,GRAMMAR_MATRIX,LANGUAGE_SUPPORT,PROJECT_STATE,TEST_MATRIX}.md`

## Decisions and evidence

ADR-0003 records the structural ID scheme, lexical scope ownership, per-language
normalization boundary and explicit non-resolution policy. IDs are independent of
byte offsets and pure line insertion, but named sibling structure is part of their
identity and can change after structural edits.

The Rust query fingerprint is
`93d2eb1fb4f9e587bd8d531c219283760d336763cbb3ada9e4b99e456a001c35` and
`rust-source-v1` produces extractor fingerprint
`865b70d6275b6a84bb61776b6c16e705fde8852b4b257d3c8e40d9f969ab3c03`.
The Go query fingerprint is
`d445256bc65be5f368651f2d4a1feb88a3c5e3f5ec5569b3434b5956e9203e5d` and
`go-source-v1` produces extractor fingerprint
`77edbd9700dd42a972bf57c09b05d452d2e99aeec6640db177824427a75b395c`.

The focused golden locks exact counts and hashes for every observation category in
eight cases. Hand-authored expectations separately assert declarations, containers,
receivers, signature fragments, aliases, calls, conditions and forbidden names; the
extractor output is not used as its own semantic ground truth.

## Commands and evidence

All successful Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME`
and explicit toolchain `PATH` from earlier phase reports because no system Cargo is
on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| `cargo run -p xtask --locked -- verify` (Phase 03 prerequisite, before edits) | 0 | prior parser/storage/lifecycle baseline clean |
| `cargo check --workspace --locked` (without pinned PATH) | 127 | `cargo` unavailable on default PATH; recorded, then rerun with pinned environment |
| `cargo check --workspace --locked` (first compile) | 101 | one adapter lifetime diagnostic; fixed before proceeding |
| `cargo check --workspace --all-targets --locked` | 0 | workspace and all targets compile |
| `cargo check -p xtask --offline` | 0 | updated lock metadata for xtask's already-locked workspace BLAKE3 dependency; no network/package change |
| `cargo test -p ca-languages --locked` | 0 | 13/13 language tests pass |
| `cargo run -p xtask --locked -- grammar-check` | 0 | nine providers and 15 total query assets compile; Rust/Go report four queries and extractor readiness |
| `cargo run -p xtask --locked -- fixtures` | 0 | 24/24 parser-kernel fixtures match reviewed golden |
| `cargo run -p xtask --locked -- rust-go-fixtures` | 0 | 8/8 focused Rust/Go expectation and exact hash lines pass |
| `jq empty config/dependency-lock.json fixtures/expectations.json fixtures/rust-go-expectations.json` | 0 | edited JSON documents valid |
| `cargo fmt --all -- --check` | 0 | formatting clean |
| `cargo clippy --workspace --all-targets -- -D warnings` (first run) | 101 | five style diagnostics; implementation refactored, no allow-list added |
| `cargo clippy --workspace --all-targets -- -D warnings` (final) | 0 | no warnings |
| `cargo test --workspace --locked` | 0 | 51 passed; one storage helper ignored and exercised by its parent test |
| `cargo run -p xtask --locked -- verify` (final) | 0 | format, clippy, 51-test workspace, grammar, 24 parser fixtures and 8 focused fixtures all pass |

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Rust/Go symbols/imports/references/calls queries compile separately | passed | grammar-check reports four query assets for each provider |
| Required declarations, containers, aliases, receiver types and calls | passed | independent focused expectations and exact golden hashes |
| Comments/string literals do not emit `phantom_call` | passed | all focused and parser fixture negatives |
| Same-name receiver methods and shadowed bindings remain distinct | passed | focused fixtures and unit test assert IDs, containers/receivers and scope IDs |
| Declaration IDs survive line insertion where tree structure is unchanged | passed | Rust and Go line-insertion unit test |
| Call targets remain unresolved and trait/interface matches are not promoted | passed | every focused call asserts `target_id=None` and `resolution=Unresolved` |
| Macro and build-configuration limitations are surfaced | passed | result limitations plus Rust cfg/Go build-condition warnings |
| Invalid edits and Unicode byte positions remain observable | passed | Rust/Go partial fixtures and `özet`/`Özet` byte-range assertions |
| Determinism, budgets, cancellation and parser reset | passed | 13 language unit tests and existing bounded parser tests |
| Earlier parser and workspace regressions | passed | 24 parser fixtures; 51-test locked workspace; final verify |
| Linux x64 native extraction | not run | target unavailable locally |
| Windows x64 MSVC native extraction | not run | target unavailable locally |

## Known limitations and risks

- This is syntax extraction, not compiler or language-server semantics. References
  are scoped observations without declaration targets. Calls are all unresolved.
- Rust macro bodies/calls are not expanded, hygiene is unavailable, build scripts
  never run and cfg activity is unknown without configuration evidence.
- Go packages are not loaded, interface satisfaction is not inferred, build tools
  never run and GOOS/GOARCH/tag activity is unknown.
- A Go import's default alias is derived from the final path segment; it does not
  prove a differing package clause in the imported source.
- Structural IDs deliberately exclude line/byte offsets but may change after named
  sibling insertions or other syntax-tree restructuring.
- The focused corpus establishes deterministic contract coverage, not real-world
  precision/recall. Parser fuzzing and native Linux/Windows execution remain pending.
- Parser work is still called synchronously at this crate boundary. The indexing
  phase must place workers behind a bounded CPU queue before protocol use.
- No MCP extraction/search capability is exposed and no observations are persisted yet.

## Blockers and safe next actions

No Phase 04 blocker on the available macOS arm64 host. Do not infer cross-platform
execution, cross-file binding or runtime dispatch from this result. Phase 05 can
implement JavaScript/JSX and TypeScript/TSX with the same reviewed capture,
scope/identity and explicit uncertainty contracts.

## Exact next prompt

`prompts/05-javascript-and-typescript.md`
