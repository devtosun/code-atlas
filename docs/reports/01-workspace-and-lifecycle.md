# Phase 01 report

State: completed  
Date: 2026-09-19  
Git revision and local diff: unavailable; supplied directory is not a Git worktree (`git status` exit 128)  
Host / target / toolchain: macOS 27.0, arm64 / `aarch64-apple-darwin`, Rust 1.98.1, Cargo 1.98.1, LLVM 22.1.8

## Scope implemented

Created the production Rust workspace with `ca-core`, `ca-languages`, `ca-storage`,
`ca-engine`, `ca-mcp`, `ca-cli` and `xtask`. The application now builds a real
`codeatlas` binary with `serve --root`, `doctor --json`, help and version surfaces.
The official rmcp server supports the modern 2026-07-28 and legacy 2025-11-25
lifecycles and advertises one implemented tool: `repository_status`.

Startup validates only the explicitly supplied absolute directory, initializes
stderr tracing and starts protocol service. It does not traverse the root, open or
migrate SQLite, acquire a writer lock, load grammars, compile queries, execute source,
contact a network service or create repository files. Status therefore returns the
genuine `not_opened` lifecycle. Language and storage crates are intentionally dormant.

Core now owns typed repository/generation IDs, half-open byte ranges, bounded result,
graph and response limits, typed errors, cooperative cancellation, validated absolute
UTF-8 roots and process lifecycle state. Engine owns the status use case; ca-mcp performs
only typed schema/protocol mapping. `doctor --json` reports the current binary,
build/runtime target, selected root and intentionally unopened capabilities.

## Files changed

- Root `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml`: seven-member workspace,
  exact production pins, Rust 1.98.1/MSRV and isolated Phase 00 spike exclusion.
- `crates/ca-core`, `ca-engine`, `ca-mcp`, `ca-cli`: domain baseline, status use case,
  official-rmcp adapter, CLI composition, doctor and real subprocess tests.
- `crates/ca-languages`, `crates/ca-storage`: minimal boundaries with no grammar or
  database implementation claims.
- `xtask`: actual format, clippy and locked-test verification with exit propagation.
- `.github/workflows/ci.yml`: native Linux, macOS ARM and Windows matrix using an
  immutable checkout action SHA and the pinned Rust toolchain.
- Architecture, dependency policy, ADR-0001, project state, test matrix, acceptance
  scenario status, static validator, startup measurement artifact and this report.

## Decisions and ADRs

- Kept the Phase 00 project and lockfile intact under `spikes/compatibility`; only
  dependencies required by this phase moved into the production lockfile.
- Pinned rmcp 3.4.0 with `macros`, `server` and `transport-io`. Typed serde/schemars
  inputs reject unknown status arguments and produce an output schema.
- Kept `ca-core` independent of rmcp, SQLite and Tree-sitter. `cargo tree -p ca-core`
  contains only the typed-error dependency and its procedural-macro implementation.
- Preserved SDK protocol semantics: an unknown legacy initialize version falls back
  to the latest supported legacy handshake rather than being echoed, while an
  unsupported modern per-request version is rejected.
- `xtask verify --inject-failure` deliberately requests a nonexistent core feature
  only after the real gates pass; Cargo's exit 101 is propagated unchanged.
- CI runner labels were checked against the official
  [runner-images inventory](https://github.com/actions/runner-images/blob/main/README.md).
  `actions/checkout` v7.0.1 was resolved to immutable commit
  `3d3c42e5aac5ba805825da76410c181273ba90b1` from the official
  [checkout releases](https://github.com/actions/checkout/releases).

ADR-0001 remains the selected local-first design and now records the Phase 01 stdio
evidence. Storage ownership and failure-mode verification remain pending by design.

## Commands and evidence

Commands used the temporary Phase 00 toolchain locations
`RUSTUP_HOME=/private/tmp/codeatlas-rustup` and
`CARGO_HOME=/private/tmp/codeatlas-cargo`; user PATH, Cargo config and Codex config
were not changed. Workspace commands ran from the repository root unless noted.

| Command | Exit code | Result | Log / artifact |
|---|---:|---|---|
| `cargo run --locked -- probe` in `spikes/compatibility` | 0 | Phase 00 grammars, rmcp compatibility and SQLite/FTS5 prerequisite rerun passed | terminal output; Phase 00 report |
| `cargo test --locked` in `spikes/compatibility` | 0 | 2 modern/legacy raw subprocess tests passed | terminal output; Phase 00 report |
| `cargo generate-lockfile` | 0 | locked 98 Rust-1.98.1-compatible packages after the final rmcp feature set | production `Cargo.lock` |
| `cargo build --workspace --locked` | 0 | debug workspace build passed | debug binary SHA-256 `6727521da4939a4ce021a06c8b0a04ef661680f8644cb317116825f560efad8d` |
| `cargo build --workspace --release --locked` | 0 | release workspace build passed | release binary SHA-256 `d7ccaaa262759c71c98353763a90a706c7b17e5a4af988b6907a0023d8daf095` |
| `cargo test -p ca-cli --test lifecycle --locked -- --nocapture` | 0 | 5 subprocess tests passed; 30-start baseline captured | `docs/reports/artifacts/01-startup-baseline.json` |
| `cargo tree -p ca-core --locked` | 0 | no rmcp, rusqlite or Tree-sitter in core | terminal output and workspace boundary test |
| `cargo run -p xtask -- verify` | 0 | format, clippy `-D warnings`, and 21 locked tests passed | terminal output |
| `cargo run -p xtask -- verify --inject-failure` | 101 (expected) | all real gates passed, then nonexistent-feature check failed and exact exit propagated | terminal output |
| `python3 scripts/validate_kit.py` | 0 | structure, links and Phase 00/01 scenario state passed | terminal output |
| `git status --short --branch` | 128 | directory is not a Git worktree | `fatal: not a git repository` |

Dependency resolution initially failed inside the network-restricted sandbox with
DNS errors. The same lock/download operations were then explicitly approved and
succeeded. This was build-time dependency access; the application startup path has
no network code.

Measured local startup baseline: 30 sequential debug-binary processes, empty unique
temporary roots, spawn to modern `server/discover` response. p50 was 4.393291 ms,
p95 was 4.875125 ms and maximum was 270.550041 ms. Every sample passed the hard
three-second integration deadline. The large maximum relative to p95 is retained as
measured evidence, not discarded or described as optimal.

## Acceptance criteria

| Criterion | Passed / failed / not run | Evidence |
|---|---|---|
| Locked debug and release workspace builds | passed | both `cargo build` commands exited 0 |
| Workspace contains all required crates with documented direction | passed | Cargo metadata and manifests; isolated spike exclusion |
| Core excludes SQLite, Tree-sitter and rmcp | passed | `cargo tree -p ca-core` plus manifest integration test |
| Modern/legacy clients list and call status before database creation | passed | real-binary raw JSON subprocess tests; temporary roots remained empty |
| Only implemented commands/tools are advertised | passed | CLI help and exact one-tool list assertions |
| Concrete schemas, annotations and malformed-argument handling | passed | list/call integration assertions and deny-unknown-fields request type |
| Invalid/unsupported protocol metadata behavior | passed | legacy fallback is not echoed; unsupported modern metadata is rejected |
| No stdout log contamination | passed | every captured stdout line parsed as JSON; startup trace observed on stderr |
| EOF and termination cleanup | passed | process exits within three seconds on EOF and is reaped after forced termination |
| Doctor reports observable facts without future claims | passed | JSON CLI integration test |
| Non-UTF-8 roots cannot be rendered lossily into protocol identity | passed | core invariant test rejects before filesystem access |
| xtask runs real gates and propagates deliberate failure | passed | verify exit 0; injected failure exit 101 |
| Startup baseline measured, not estimated | passed | 30 samples and committed JSON artifact |
| Linux x64 and Windows x64 native run | not run | jobs configured, but no CI run exists from this non-Git directory |

## Known limitations and risks

- Only macOS arm64 executed. The Linux and Windows jobs are configuration, not native
  execution evidence, until a hosted workflow run completes.
- Root validation in Phase 01 proves absolute/existing/directory only. Canonical,
  component-aware containment, symlink/junction escape rejection and authorized
  resource URI handling belong to Phase 02.
- No production database, migration, WAL, lock, scanner, parser, index, watcher,
  search, graph, memory, resource or prompt implementation exists yet.
- The startup measurement uses the debug binary on one lightly characterized local
  host and includes process scheduling/cache effects. It is a baseline and safety
  check, not a release performance guarantee.
- Forced termination proves the client-owned server process is reaped. Phase 01
  starts no subprocesses; later worker/process isolation changes will need explicit
  descendant cleanup tests.
- The supplied directory has no Git metadata, so revision, dirty-state and CI-run
  evidence cannot be recorded.

## Blockers and safe next actions

No Phase 01 acceptance blocker remains. Before claiming cross-platform support,
run the committed workflow on GitHub and retain the native job results. Phase 02 may
now add safe repository authorization and SQLite lifecycle without changing the
status-first protocol boundary.

## Exact next prompt

`prompts/02-safe-repositories-and-storage.md`
