# Phase 11 report — production MCP tool surface and end-to-end contracts

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git
worktree (git status --short exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 /
aarch64-apple-darwin; Rust 1.98.1
(48a229ceaefd4985c50990b14116b6d856af0985c, LLVM 22.1.8)

## Scope implemented

- Exposed repository_status, index_repository, job_status, cancel_job,
  search_symbols, get_symbol, find_references, trace_calls, get_file_outline,
  read_code, get_repo_map, analyze_impact and build_context through official rmcp.
- Added closed serde/schemars inputs with typed language/mode/direction enums,
  documented limits, unknown-field rejection and a generated common output schema.
  Read/mutation, destructive, idempotent and closed-world annotations reflect actual
  behavior.
- Added a transport-neutral backend port. Thin handlers dispatch blocking work
  through Tokio's blocking executor; policy remains in engine use cases and no SQL
  moved into ca-mcp.
- Split indexing into durable prepare and run-prepared operations. MCP returns a job
  ID before traversal, retains owner/cancellation/worker handles, deduplicates active
  request keys, reports retryable writer contention and joins workers at EOF.
- Mapped execution failures to structured isError envelopes with stable codes and
  JSON text fallback. Malformed requests and unsupported protocol metadata remain
  rmcp protocol errors.
- Added a final serialized CallToolResult cap with transport reserve. Independent
  subprocess assertions cover complete emitted tool frames at or below 65,536 bytes.
- Added a depth bound to repository maps so the public contract's depth input has
  real engine behavior rather than an ignored field.
- Added a manual one-session Codex smoke recipe that uses command-line overrides and
  does not edit user configuration.

## Files changed

- MCP schemas, handlers, envelope/errors and SDK lifecycle:
  crates/ca-mcp/src/lib.rs and crates/ca-mcp/Cargo.toml
- Engine job preparation and repository-map depth:
  crates/ca-engine/src/indexing.rs and crates/ca-engine/src/retrieval.rs
- CLI backend/composition/capability reporting:
  crates/ca-cli/src/mcp_backend.rs and crates/ca-cli/src/main.rs
- Full-binary modern/legacy tests:
  crates/ca-cli/tests/lifecycle.rs and crates/ca-cli/tests/cli.rs
- Lock and evidence metadata: Cargo.lock, config/dependency-lock.json,
  tests/acceptance-scenarios.json and scripts/validate_kit.py
- Contracts/state/guides: README.md, README.tr.md, docs/ARCHITECTURE.md,
  docs/MCP_CONTRACT.md, docs/PROJECT_STATE.md, docs/SECURITY_PRIVACY.md,
  docs/TEST_MATRIX.md and docs/manual/11-codex-smoke.md
- Decision: docs/adr/0010-typed-mcp-tools-and-detached-jobs.md

## Decisions and ADRs

ADR-0010 records the typed protocol boundary, blocking-backend dispatch, detached
but retained application-job lifecycle, stable application errors and final response
accounting. The root remains a trusted startup argument and is absent from tool
schemas. Application jobs deliberately do not enable or claim standardized MCP
Tasks.

No database migration or newly resolved third-party package was needed. serde_json
and Tokio were already pinned workspace dependencies; declaring them directly in
ca-mcp only updated that package's Cargo.lock dependency list.

## Commands and evidence

All Cargo commands used the pinned temporary RUSTUP_HOME/CARGO_HOME and explicit
toolchain PATH because no system Cargo is on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| cargo run -p xtask --locked -- verify (Phase 10 prerequisite) | 0 | prior workspace and focused fixture gates clean |
| git status --short | 128 | directory is not a Git worktree |
| cargo check --workspace --locked (first attempt) | 101 | lockfile needed direct existing dependency declarations; refreshed offline |
| cargo check --workspace --offline (first attempt) | 101 | rmcp tool-router macro cannot discover methods emitted by an inner macro; router macro changed to emit the whole impl |
| cargo check --workspace --offline | 0 | production tool/backend composition compiled |
| cargo test --workspace --locked (first attempt) | 101 | Phase 01 status-only lifecycle expectations correctly failed after thirteen tools; replaced with Phase 11 contracts |
| cargo test -p ca-cli --test lifecycle --locked | 0 | nine full-binary lifecycle/tool/job tests pass |
| codex --version | 0 | installed codex-cli 0.154.0 recorded |
| codex --help; codex mcp --help; codex mcp add --help | 0 | manual recipe based on the installed CLI surface; PATH-alias warning was nonfatal |
| cargo fmt --all -- --check | 0 | formatting clean |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 0 | no warnings |
| cargo test --workspace --locked | 0 | 87 tests passed; two child helpers ignored and exercised by parent tests |
| cargo run -p xtask --locked -- verify | 0 | workspace and focused fixture gates pass |
| python3 scripts/validate_kit.py | 0 | phase/scenario/dependency/report/link state is consistent |
| jq empty config/dependency-lock.json tests/acceptance-scenarios.json | 0 | JSON evidence files are valid |

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Thirteen real tools work with persisted fixture data | passed | modern and legacy all-tools subprocess tests |
| Both protocol eras have valid discovery/schema/results | passed | 2026-07-28 and 2025-11-25 full-binary clients |
| Discovery/list/status stay independent of indexing and locks | passed | empty-root status, active-job status and follower contention tests |
| Index returns before traversal and supports durable status | passed | queued response plus polling to real terminal job state |
| Idempotency and cancel/commit races are truthful | passed | active same-key reuse, different-key WRITER_BUSY and cancelled/completed terminal assertion |
| Unknown fields/enums/ranges and foreign IDs fail | passed | protocol and stable application-error assertions |
| Source/root boundaries and freshness stay enforced | passed | no root input plus existing reader/hash tests exercised through read_code |
| Whole tool frames remain valid and bounded | passed | every emitted subprocess line parses as JSON; tool responses are at most 65,536 bytes |
| EOF leaves no orphan worker or mixed active generation | passed | active 300-file job is cancelled and joined before successful process exit |
| Actual installed Codex MCP execution | not run | version/help inspected; no config or model session was authorized |
| Linux x64 native protocol/job behavior | not run | target unavailable locally |
| Windows x64 MSVC native protocol/job behavior | not run | target unavailable locally |

Acceptance scenarios P29 and P30 are PASS on the available macOS arm64 host.

## Known limitations and risks

- Retrieval and graph results remain syntax-informed. They do not prove runtime
  dispatch, exhaustive impact or dead code.
- One database has one writer. A competing mutation receives retryable WRITER_BUSY;
  there is no daemon or remote queue.
- The common output schema fixes the envelope but leaves tool-specific data as JSON.
  End-to-end tests validate each tool's actual data; later compatible versions may
  publish narrower per-tool data schemas.
- The final adapter reserves 512 bytes for the JSON-RPC frame. Tests establish the
  cap for normal bounded numeric request IDs; an adversarially enormous JSON-RPC ID
  is protocol framing input owned by rmcp and is not counted as tool content.
- Background indexing is recoverable durable work, not resumable MCP Tasks. Process
  death can mark a staging job interrupted on reopen; it never activates an
  incomplete generation.
- No actual installed-Codex model session ran. The full-binary raw clients establish
  SDK wire behavior, not Codex UI/integration behavior.
- Linux and Windows native behavior, parser fuzzing, watcher reconciliation,
  power-loss durability and performance measurements remain unexecuted here.

## Blockers and safe next actions

No Phase 11 blocker on the available macOS arm64 host. Phase 12 may add bounded file
watching and reconciliation while preserving the asynchronous job, writer-owner and
atomic-generation contracts. It must not broaden root authority or turn watcher
events into direct truth without rescan/hash reconciliation.

## Exact next prompt

prompts/12-watch-and-incremental.md
