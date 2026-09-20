# Phase 00 report

State: completed  
Date: 2026-09-19  
Git revision and local diff: unavailable; supplied directory is not a Git worktree (`git status` exit 128)  
Host / target / toolchain: macOS 27.0 (26A428), arm64 / `aarch64-apple-darwin`, Rust 1.98.1, Cargo 1.98.1, Apple Clang 17.0.0

## Scope implemented

Created an isolated, runnable Rust compatibility spike. One executable links every
required Tree-sitter grammar, parses nonempty fixtures, compiles and executes a query
for Dart, C#, Rust, Go, Java, JavaScript, JSX, TypeScript and TSX, and separately
tests modern Dart syntax. The same spike provides a tiny official-rmcp stdio server;
an independent raw-JSON subprocess test client exercises modern and legacy wire
lifecycles. A bundled SQLite probe enforces the safety floor and performs a real
FTS5 insert/search/delete transaction.

The spike is disposable evidence, not the CodeAtlas application. No indexer, graph,
repository traversal, daemon, network runtime, user configuration or full tool set
was implemented.

## Files changed

- `spikes/compatibility/`: pinned Cargo project, lockfile, fixtures, compiled queries,
  rmcp server/client integration test and raw protocol frames.
- `config/dependency-lock.json`: machine-readable toolchain, package, source commit,
  checksum, ABI, query hash, SQLite and native-platform evidence.
- `docs/GRAMMAR_MATRIX.md`, `docs/PROJECT_STATE.md`, `docs/TEST_MATRIX.md`: executed
  Phase 00 status and limitations.
- This report.

## Decisions and ADRs

- Pinned Rust 1.98.1 and Tree-sitter 0.27.0 (runtime grammar ABI interval 13–15).
- Used only published crates with exact versions and Cargo checksums. No Git main,
  vendored grammar, unsafe language cast, regex fallback or runtime download.
- Resolved published Dart crate 0.2.0 to
  `nielsenko/tree-sitter-dart@b57d734c84f510bbd524097902cab671e4dbfca9`.
  `UserNobody14/tree-sitter-dart` is not the published crate source used here.
- Selected official rmcp 3.4.0, features `server` and `transport-io`. Raw tests keep
  the 2026-07-28 discovery/per-request metadata contract separate from the
  2025-11-25 initialize lifecycle.
- Selected rusqlite 0.40.2 `bundled-full`; observed SQLite 3.53.2, so no safety-floor
  exception ADR is needed. No other architecture decision changed.

## Commands and evidence

Commands below were run from `spikes/compatibility` unless a path says otherwise.
The temporary toolchain used `RUSTUP_HOME=/private/tmp/codeatlas-rustup` and
`CARGO_HOME=/private/tmp/codeatlas-cargo`; no user PATH/profile/config was changed.

| Command | Exit code | Result | Log / artifact |
|---|---:|---|---|
| `rustc -Vv` | 0 | Rust 1.98.1, commit `48a229ceaefd4985c50990b14116b6d856af0985`, LLVM 22.1.8 | this report / dependency lock |
| `cargo -Vv` | 0 | Cargo 1.98.1, target `aarch64-apple-darwin` | this report / dependency lock |
| `cargo generate-lockfile --manifest-path spikes/compatibility/Cargo.toml` | 0 | locked 140 Rust-1.98-compatible packages | `spikes/compatibility/Cargo.lock` |
| `cargo run --locked -- probe` | 0 | all grammar parses/queries pass; SQLite 3.53.2 FTS5 transaction passes | stdout summarized below |
| `COMPAT_ARTIFACT_DIR=artifacts cargo test --workspace --locked` | 0 | 2 subprocess stdio lifecycle tests passed | `spikes/compatibility/artifacts/*.jsonl` |
| `cargo fmt --all -- --check` | 0 | formatting clean | command output |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | no warnings | command output |
| `python3 scripts/validate_kit.py` (repository root) | 0 | static kit checks passed | command output |
| JSON parse of `config/dependency-lock.json` | 0 | valid JSON | command output |
| `git status --short --branch` (repository root) | 128 | directory is not a Git worktree | exact diagnostic: `fatal: not a git repository` |
| `command -v cargo-audit` | 1 | command is not installed; no finding was suppressed | host inspection |
| `command -v cargo-deny` | 1 | command is not installed; no finding was suppressed | host inspection |

During implementation, one compile run failed on the Tree-sitter 0.27 `captures()`
accessor and one MCP run exposed incorrectly placed modern `_meta`. Both were fixed;
the final locked commands above passed without weakening assertions.

Observed runtime evidence:

- Runtime ABI interval: 13–15. Grammar ABI: Dart/C#/Rust/Go/JavaScript/JSX 15;
  Java/TypeScript/TSX 14.
- Every basic fixture reported `parse_has_error=false`; each query captured its
  hand-selected expected name. The modern Dart fixture also reported no parse error
  for extension type, record, relational-pattern and switch-expression syntax.
- Modern raw frames contain `server/discover`, required metadata inside each
  `params._meta`, and `resultType: "complete"`. Legacy frames contain initialize,
  initialized notification, list and call without the modern discriminator.
- Bundled SQLite reported 3.53.2 and `ENABLE_FTS5`; the transaction returned one
  search hit and zero rows after deletion. All compile options are machine-recorded.
- Installed Codex version: `codex-cli 0.154.0`. No claim is made about the lifecycle
  negotiated by that external executable because this spike did not introspect it.

## Acceptance criteria

| Criterion | Passed / failed / not run | Evidence |
|---|---|---|
| One build contains all seven languages plus JSX/TSX | passed | single locked binary links and executes all bindings |
| Basic fixtures have no unexpected ERROR/MISSING nodes and queries run | passed | probe output; matrix and fixture/query files |
| Every grammar has verified provenance, ABI and pin | passed | Cargo.lock, package archives, grammar matrix, dependency lock |
| Modern and legacy MCP lifecycle frames are version-correct | passed | 2 subprocess tests and four raw JSONL artifacts |
| Bundled SQLite meets known-fix floor and FTS5 works | passed | observed 3.53.2 and transaction probe |
| Reproducible `--locked` rerun exists | passed | spike README and final commands |
| Linux x64 native execution | not run | native runner unavailable |
| Windows x64 MSVC native execution | not run | native runner unavailable |

## Known limitations and risks

- The syntax fixtures are deliberately small compatibility probes. They do not prove
  the full extraction, incomplete-source, Unicode/CRLF, cancellation/budget or
  semantic-resolution contract required by later language phases.
- Only macOS arm64 was executed. Native compiler/linker/SQLite/stdio behavior on
  Linux x64 and Windows x64 MSVC is still unknown.
- The query files prove compilation/execution and dialect selection; they are not the
  production symbol/import/reference/call queries planned for phases 03–07.
- Cargo audit/deny tooling was unavailable. Exact crates and checksums are recorded,
  but a vulnerability/license policy scan still belongs in later hardening work.
- The directory has no `.git` repository, so revision/dirty-state evidence cannot be
  provided until the user places the kit in a Git worktree.

## Blockers and safe next actions

No Phase 00 acceptance blocker remains. Add Linux and Windows native CI evidence in
the later release matrix; do not reinterpret this macOS run as cross-platform proof.

## Exact next prompt

`prompts/01-workspace-and-lifecycle.md`
