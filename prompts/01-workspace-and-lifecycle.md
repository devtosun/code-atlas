# Phase 01 — Rust workspace, CLI and fast MCP lifecycle

$ca-phase-driver $ca-rust-architecture $ca-mcp-contract

## Task and scope
Create the production workspace using proven dependency choices and a minimal real stdio server that cannot be blocked by repository startup work.

## Prerequisites
Phase 00 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/ARCHITECTURE.md`
- `docs/MCP_CONTRACT.md`
- `docs/DEPENDENCY_POLICY.md`

## Implementation steps
1. Create ca-core, ca-languages, ca-storage, ca-engine, ca-mcp, ca-cli and xtask
   with the documented dependency direction. Pin toolchain/MSRV and shared dependencies;
   preserve phase-00 evidence. Keep dormant modules minimal instead of fake implementations.
2. Add validated IDs/ranges/limits, typed domain errors, cancellation context and
   a small process state model. Build CLI --help/--version and `serve --root`.
   Only list implemented commands in help; future commands stay in the design docs.
3. Use official rmcp for modern/legacy transport behavior and a genuine
   repository_status tool reporting not_opened. No fake index/search tools yet.
4. Set tracing to stderr. A minimal trusted configuration read must not trigger
   traversal, database opening/migration, grammar query compilation or owner locks.
   Start protocol service first; slow repository work is later and explicit.
5. Add `doctor --json` for currently observable binary/toolchain/config facts without
   claiming future storage tests exist. No network call or model download on startup.
6. Implement `cargo run -p xtask -- verify` to run existing format/lint/test gates
   with correct exit-code propagation. Add a basic CI workflow on primary native OSes
   where runners are actually available; pin actions according to reviewed policy.
7. Test the real subprocess: both lifecycle eras, status, tools/list, malformed
   arguments/version, stdout-only protocol, EOF and termination cleanup.

## Acceptance gates
- Release/debug binaries build with --locked and workspace tests pass.
- Core has no SQLite, Tree-sitter or rmcp dependencies; handler business logic is thin.
- Modern/legacy client can list/call status before any database exists.
- Logs contain no stdout contamination; EOF ends the process without children.
- The implemented xtask runs actual gates and fails on a deliberately failing check.
- Startup timing is measured and reported as a baseline, not declared optimal.

## Out of scope
No bulk index, syntax extraction placeholders, memory notes, HTTP transport or Codex installation.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/01-workspace-and-lifecycle.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
