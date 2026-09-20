# Phase 08 report — persistent indexing pipeline and cancellable jobs

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git
worktree (`git status --short` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 /
`aarch64-apple-darwin`; Rust 1.98.1
(`48a229ceaefd4985c50990b14116b6d856af0985c`, LLVM 22.1.8)

## Scope implemented

- Added a typed engine indexing use case behind storage and extraction ports. It
  completes safe traversal, rereads/hashes selected bytes, uses a bounded source
  queue and capped worker-local parsers, and persists results in bounded batches.
- Added exact incremental reuse checks for content, configuration, grammar, query
  and extractor fingerprints. mtime and size are never correctness keys; the test
  suite changes content while restoring the original mtime.
- Migrated SQLite to schema version 3 with immutable language/coverage metadata,
  owned symbols/scopes/imports/references/call sites/conditions/diagnostics,
  generation coverage/config fields and durable index jobs.
- Added queued/scanning/parsing/resolving/committing/completed plus failed,
  cancelled and interrupted job states; real counters; request-key deduplication;
  idempotent durable cancellation; and nonterminal-owner recovery.
- Candidate generations activate atomically only after a complete traversal.
  Failed/cancelled/interrupted candidates never displace the active generation;
  abandoned/unreferenced versions have a bounded writer-owned GC path.
- Mapped all nine language/dialect extraction results into immutable SQLite rows.
  Declaration facts also populate generation-scoped FTS documents. Coverage remains
  explicitly syntax-only; no semantic or dummy resolved edges are created.
- Added engine-backed `codeatlas index`, `status` and `doctor` commands with JSON
  modes. `index` supports incremental/full operation and an optional request key.
  A follower mutation returns the existing bounded `write owner is busy; retry`
  error and never starts a daemon.
- Kept the MCP server status-only. Phase 08 does not advertise placeholder index,
  job, cancellation, search or memory tools.

## Files changed

- Core/engine: `crates/ca-core/src/lib.rs`,
  `crates/ca-engine/src/{lib,indexing,repository}.rs`
- Language boundary: `crates/ca-languages/src/{registry,worker}.rs`
- Persistence: `crates/ca-storage/src/{schema,storage}.rs`
- CLI composition/tests: `crates/ca-cli/{Cargo.toml,src/main.rs,src/indexer.rs,tests/cli.rs}`
- Lock/state/contracts: `Cargo.lock`, `config/dependency-lock.json`, `README.md`,
  `README.tr.md`, `docs/{ARCHITECTURE,DATA_MODEL,DEPENDENCY_POLICY,GRAMMAR_MATRIX,LANGUAGE_SUPPORT,MCP_CONTRACT,PROJECT_STATE,SECURITY_PRIVACY,TEST_MATRIX}.md`
- Static evidence: `scripts/validate_kit.py`, `tests/acceptance-scenarios.json`
- Decision: `docs/adr/0007-persistent-index-jobs.md`

## Decisions and evidence

ADR-0007 records the engine-port boundary, bounded queue/worker design, exact reuse
key, durable state machine, cooperative cancellation and atomic activation policy.
`ca-engine` depends on neither SQLite nor Tree-sitter; concrete adapters live at the
CLI composition root. The phase adds no new third-party package and keeps the
committed production lock.

Traversal must be complete before deletion inference. Symlink exclusions and
policy-excluded binary, invalid-UTF-8 or oversized inputs remain visible warnings;
unreadable traversal, resource-limit, changed-during-read, storage and cancellation
failures abort activation. Per-file parser failure rows retain explicit coverage and
diagnostics and may activate only as `ready_with_warnings`.

The killed-writer test starts a real child writer, leaves a building generation and
uses `process::exit` so Rust destructors do not run. Reopening marks that job
interrupted and its generation abandoned while preserving the healthy active
snapshot. This is process-crash recovery evidence, not a claim about every power-loss
or filesystem failure mode.

## Commands and evidence

All Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME` and explicit
toolchain `PATH` because no system Cargo is on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| `cargo run -p xtask --locked -- verify` (Phase 07 prerequisite, before edits) | 0 | 62 tests, 36 query assets and all prior fixture suites clean |
| `cargo test -p ca-storage --locked --no-run` | 0 | schema v3 and expanded storage actor compile |
| `cargo test -p ca-storage --locked` (first storage slice) | 0 | 9 passed, one child helper ignored |
| manual `codeatlas index/status/index --json` against an isolated root | 1 on second index | exposed a cross-process second-granularity job-ID collision; changed IDs to include nanoseconds, PID and process-local sequence |
| repeated manual `codeatlas index/index/status --json` | 0 | first parse persisted one file; restarted no-op reused one; status showed active generation/fact/symbol |
| `cargo test -p ca-cli --locked --test cli -- --nocapture` | 0 | seven CLI tests pass, including nine language/dialect seeds, restart and incremental mutations |
| `cargo test -p ca-cli -p ca-storage --locked` | 0 | durable worker cancellation plus CLI, MCP lifecycle, lock/recovery and storage suites pass |
| `cargo test -p ca-storage --locked killed_writer_is_recovered_without_displacing_active_data -- --nocapture` | 0 | abrupt child writer exit recovered without replacing active data |
| `cargo fmt --all -- --check` (first final attempt) | 1 | formatting-only differences; applied `cargo fmt --all` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | no warnings |
| `cargo test --workspace --locked` (first final attempt) | 101 | one scanner regression: policy exclusions had been marked traversal-incomplete; restored the specified distinction while retaining visible diagnostics |
| `jq empty config/dependency-lock.json` | 0 | dependency evidence JSON valid |
| `cargo run -p xtask --locked -- verify` (first final attempt) | 101 | cancellation test could request after parsing state but before worker entry; added an explicit worker-start latch and bounded deadline |
| `python3 scripts/validate_kit.py` (first run) | 1 | validator still required pre-Phase-03 unexecuted fixtures and Phase-02 state; advanced its evidence checks through completed Phase 08 |
| `cargo fmt --all -- --check` (final) | 0 | formatting clean |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` (final) | 0 | no warnings |
| `cargo test --workspace --locked` (final) | 0 | 69 passed; two child helpers ignored and executed by their parent tests |
| `cargo run -p xtask --locked -- verify` (final) | 0 | format, clippy, workspace tests, 36 query assets and all focused fixture suites pass |
| `python3 scripts/validate_kit.py` (final) | 0 | phase/fixture/scenario manifests, TOML examples, Markdown links and dependency record are consistent through Phase 08 |

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| CLI indexes all supported seeds and survives restart | passed | nine Dart/C#/Rust/Go/Java/JS/JSX/TS/TSX files; status in a new process reports nine active files and persisted facts/symbols |
| No-op reuse is deterministic | passed | second incremental job reuses 9/9, parses 0 and persists 9 memberships; `--full` reparses 9/9 |
| Modified/deleted/renamed and identical-mtime changes are correct | passed | exact test counters: three discovered, one reused, two parsed, two deleted and three persisted |
| Failed/cancelled candidate retains prior active generation | passed | unreadable traversal rollback, incomplete-generation storage test and active-worker cancellation test |
| FTS/facts/deletions match activated membership | passed | declaration-derived FTS trigger test plus activated empty-generation fact/FTS assertions |
| Busy owner is actionable | passed | owner/follower and child-process lock tests; index-job creation returns `WriterBusy` with 250 ms retry hint |
| Job progress/request dedup are durable | passed | exact CLI counters plus storage request-key and cancellation idempotency tests |
| Cancellation reaches parser work | passed | active slow worker observes the shared cancellation token set from the durable job flag |
| Concurrent readers and killed writer preserve consistency | passed | pinned old snapshot across activation; real abruptly exiting child writer recovery |
| Memory work is structurally bounded | passed | validated worker/queue/batch/warning caps, bounded sync channel, source-only tasks and batch persistence; no AST corpus retained |
| Linux x64 native indexing | not run | only `aarch64-apple-darwin` is installed locally |
| Windows x64 MSVC native indexing | not run | native runner unavailable locally |

## Known limitations and risks

- Phase 08 is syntax-only. `resolving` is an honest lifecycle checkpoint but performs
  no cross-file binding; `resolved_edges` is not created and name-resolution
  readiness remains false.
- The CLI index call is synchronous. Durable asynchronous MCP index/job/cancel tools
  remain a later protocol phase and are not advertised.
- No file watcher or event reconciliation exists. Every CLI job performs a bounded
  complete traversal before deletion inference.
- Policy-excluded oversized, binary and invalid-UTF-8 files are diagnosed and absent
  from the new generation; a traversal/reader race or unreadable directory aborts the
  entire candidate. Parser limits remain cooperative, not wall-clock hard isolation.
- The bounded design has deterministic cap/backpressure tests, but Phase 08 does not
  claim measured 10k/100k throughput or peak RSS. Those measurements remain Phase 14
  work under the documented hardware/corpus protocol.
- Immediate abandoned-generation GC can invalidate references to failed candidate
  generations; durable job error/progress remains the supported failure record.
- Native Linux/Windows behavior, parser fuzzing, hostile-co-tenant directory races
  and power-loss durability remain unproven on this host.

## Blockers and safe next actions

No Phase 08 blocker on the available macOS arm64 host. Do not infer cross-platform
execution, semantic resolution, public retrieval, watcher behavior, asynchronous MCP
jobs or production-scale performance from this evidence. Phase 09 may add bounded
cross-file resolution and graph evidence over the activated immutable generation.

## Exact next prompt

`prompts/09-resolution-and-graph.md`
