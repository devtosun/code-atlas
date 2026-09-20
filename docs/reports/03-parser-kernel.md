# Phase 03 report — parser kernel

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git worktree (`git status` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 / `aarch64-apple-darwin`; Rust 1.98.1 (`48a229ceaefd4985c50990b14116b6d856af0985`, LLVM 22.1.8)

## Scope implemented

- Promoted Tree-sitter 0.27.0 and the exact seven Phase 00 grammar package pins into
  the production workspace lock.
- Added nine explicit production providers: Dart, C#, Rust, Go, Java, JavaScript,
  JSX, TypeScript and TSX. JSX is an explicit JavaScript-grammar dialect; TSX uses
  the dedicated TSX grammar value.
- Added embedded, node-types-reviewed `symbols.scm` declaration-anchor queries and
  enforced the `declaration.<kind>.name` capture contract. Invalid queries report
  language, asset name, line, column and compiler message.
- Added worker-local parsers and compiled queries, valid-UTF-8 input policy, original
  byte preservation, owned observations, ERROR/MISSING diagnostics and independent
  parser/extractor capability flags.
- Bounded source bytes, native progress callbacks, syntax traversal, diagnostics,
  in-progress query matches and capture output. Added cooperative cancellation and
  progress hooks, with parser reset after callback-driven interruption. These are
  cooperative budgets, not hard timeouts or a native-code sandbox.
- Added deterministic BLAKE3 source, grammar and query fingerprints. Grammar
  fingerprints cover package/version/ABI/node-types metadata; they do not attest the
  native object file.
- Executed all 24 original fixtures against hand-authored minimum declarations and
  parse expectations. Added a manually reviewed golden containing exact byte ranges
  and source/grammar/query hashes. There is no snapshot-update command.
- Added `xtask grammar-check` and `xtask fixtures`; both use distinct nonzero failure
  codes and are included after the standard checks in `xtask verify`.

## Files changed

- Production kernel: `crates/ca-languages/src/{lib,model,registry,worker}.rs`
- Embedded queries: `crates/ca-languages/queries/*/symbols.scm`
- Dependency promotion: workspace `Cargo.toml`, `Cargo.lock`,
  `crates/ca-languages/Cargo.toml`
- Gates/harness: `xtask/{Cargo.toml,src/main.rs}`,
  `crates/ca-cli/tests/workspace.rs`
- Reviewed corpus: `fixtures/expectations.json`, `fixtures/parser-kernel.golden`,
  `fixtures/README.md`
- Decisions/state: `config/dependency-lock.json`, `docs/adr/0002-parser-kernel-boundary.md`,
  `docs/{ARCHITECTURE,GRAMMAR_MATRIX,LANGUAGE_SUPPORT,PROJECT_STATE,TEST_MATRIX}.md`

## Decisions and ADRs

ADR-0002 records worker-local parser/query ownership, cooperative rather than hard
cancellation, bounded work/output, original-byte range policy, static trusted
grammars, fingerprint semantics and honest capability separation.

The full owned result already has declarations, scopes, imports, references, call
sites, diagnostics and coverage. Phase 03 populates only reviewed declaration
anchors and emits `parser_kernel_extraction_pending`; all extraction readiness flags
remain false. Phases 04–07 must replace this kernel subset with complete
language-specific extraction rather than treating the anchors as support.

## Commands and evidence

All Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME` documented in
the prior reports because no system Rust installation is available.

| Command | Exit code | Result | Log / artifact |
|---|---:|---|---|
| `cargo run -p xtask --locked -- verify` (Phase 02 prerequisite, before edits) | 0 | prior 37-test workspace and lifecycle/storage baseline clean | terminal output |
| `cargo check -p ca-languages` (sandboxed first dependency resolution) | 101 | crates.io DNS blocked; recorded as failed, not passed | terminal diagnostic `Could not resolve host: index.crates.io` |
| `cargo check -p ca-languages` (approved network retry) | 0 | exact pinned grammar dependencies resolved and production lock updated | `Cargo.lock` |
| `cargo run -p xtask --locked -- grammar-check` | 0 | all nine providers load; all embedded queries/capture contracts compile; ABIs 14/15 and full fingerprints printed | `config/dependency-lock.json` |
| `cargo run -p xtask --locked -- fixtures` (before reviewed golden existed) | 4 | intentionally failed closed and printed candidate snapshot | terminal output |
| `cargo run -p xtask --locked -- fixtures` (after byte/range review) | 0 | 24/24 expectations and golden lines pass | `fixtures/parser-kernel.golden` |
| `cargo test -p ca-languages -p xtask --locked` | 0 | 10 parser-kernel + 3 harness/gate tests pass | terminal output |
| `jq empty config/dependency-lock.json fixtures/expectations.json` | 0 | both edited JSON documents valid | terminal output |
| `cargo fmt --all -- --check` | 0 | formatting clean | terminal output |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | no warnings | terminal output |
| `cargo test --workspace --locked` | 0 | workspace tests pass; one storage helper remains intentionally ignored and is exercised by its parent test | terminal output |
| `cargo run -p xtask --locked -- verify` | 0 | standard gates plus grammar and fixture gates pass | terminal output |

## Acceptance criteria

| Criterion | Passed / failed / not run | Evidence |
|---|---|---|
| All required grammars and dialects load through the production registry | passed on macOS arm64 | `grammar-check`; registry unit test |
| Embedded queries compile; invalid query errors identify language and asset | passed | query compilation unit test and `grammar-check` |
| Worker-local parsing returns deterministic owned facts and original UTF-8 ranges | passed | ownership/determinism plus BOM/Unicode/CRLF tests |
| Source, capture, syntax traversal, query-match and native progress work are bounded | passed | limit tests and typed coverage/diagnostics |
| Cancellation/progress interruption resets parser state | passed | interrupted deep parse followed by successful reuse |
| Empty, incomplete, ERROR/MISSING and invalid-encoding behavior is explicit | passed | malformed/encoding unit test and `partial/broken.ts` golden |
| Fixture harness rejects missing declarations and false-positive observations | passed | xtask negative unit test; all `phantom_call` fixture negatives |
| Original seed files produce reviewed deterministic goldens with hashes | passed | 24/24 `xtask fixtures`; `parser-kernel.golden` |
| Parser and extractor readiness are reported independently and accurately | passed | all nine grammar-check records report true/false respectively |
| Linux x64 native grammar build/execution | not run | target unavailable locally |
| Windows x64 MSVC native grammar build/execution | not run | target unavailable locally |

## Known limitations and risks

- The declaration queries are Phase 03 anchors, not complete extractors. Scopes,
  imports/exports, references, calls, containers, signatures and semantic binding
  remain pending. No user-facing parser/index capability is advertised over MCP.
- Tree-sitter callbacks provide cooperative checkpoints, not hard wall-clock
  preemption. Native grammars are trusted dependencies; process isolation and
  parser fuzzing remain later hardening work.
- `ParserWorker` establishes worker-local ownership. The bounded async-to-blocking
  worker queue is added when indexing orchestration is implemented; protocol paths
  still do not invoke parsing.
- Only macOS arm64 executed this phase. Native Linux/Windows evidence is unavailable
  because only `aarch64-apple-darwin` is installed and this directory is not a Git
  worktree from which the configured CI can be dispatched.
- `cargo audit` and `cargo deny` remain unavailable.

## Blockers and safe next actions

No Phase 03 blocker on the available host. Do not infer cross-platform execution or
complete language support from this result. The safe next phase is the requested
Rust and Go extraction work, preserving the current capability flags until each
adapter meets its own positive, negative, scope/import/reference/call and partial
source gates.

## Exact next prompt

`prompts/04-rust-and-go.md`
