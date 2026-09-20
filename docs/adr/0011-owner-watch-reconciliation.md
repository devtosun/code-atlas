# ADR-0011: owner-scoped watch hints with periodic reconciliation
Status: accepted
Date: 2026-09-19
Phase: 12

## Context and evidence

Filesystem notification streams differ by platform, editor and mounted filesystem.
They can coalesce, reorder or omit events, and atomic saves commonly appear as
create/rename/remove sequences. CodeAtlas must not translate those events directly
into database truth or permit two processes to publish competing generations.

The existing Phase 08–11 pipeline already supplies the correctness boundary: a safe
ignore-aware scan, content hashing, immutable candidate generations, full graph
re-resolution, cooperative cancellation and atomic activation. Phase 12 needs to
schedule that pipeline without delaying MCP discovery or adding a daemon.

## Decision

Watching is opt-in through trusted `serve --watch` startup state and starts only
after an explicit index request acquires the OS write-owner lock. The process retains
that owner storage handle and owns one coordinator. Followers remain query-only and
do not create a watcher.

notify 8.2.0 provides the recommended native backend. If initialization fails and
fallback is allowed, or when `--watch-poll` is explicitly selected, PollWatcher uses
content comparison. Callbacks use `try_send` into a bounded synchronous queue;
overflow is counted. A 250 ms default debounce coalesces hints. Independent periodic
reconciliation defaults to 60 seconds and always performs the authoritative full
scan/hash comparison.

Each reconciliation runs an ordinary durable incremental job behind the same
serialization gate as manual jobs. Changed files are fully reparsed and the complete
candidate graph is re-resolved. Tree-sitter subtree reuse is not implemented. Status
exposes the backend, pending/reconciling state, queue/overflow counters, trigger,
errors and degraded filesystem-event guarantee. EOF cancels jobs, wakes and joins the
watcher, joins workers and then releases ownership.

## Alternatives considered

- Applying create/modify/remove events directly to SQLite was rejected because event
  delivery and editor rename patterns are not authoritative.
- A mandatory daemon was rejected because the product lifecycle is client-owned
  stdio and must end on EOF.
- Unbounded channels were rejected because source bursts need observable backpressure.
- Tree-sitter old-tree reuse was deferred because arbitrary editor changes require
  exact `InputEdit` sequences and equivalence evidence.
- Polling only was rejected as the default because native hints reduce expected
  latency and scanning, while periodic polling/reconciliation remains the safety net.

## Trade-offs and failure modes

Periodic reconciliation can perform full repository discovery and full graph
resolution even when native events are quiet. PollWatcher with content comparison
adds I/O, especially on mounted paths. A queue overflow loses event detail but not
eventual correctness because it is recorded and reconciliation scans actual state.
Process death can interrupt a candidate but cannot replace the active generation;
the next owner performs existing interrupted-job recovery.

The macOS FSEvents backend initialized in the available managed host but did not
deliver a callback during the short native burst test. The explicit PollWatcher and
periodic paths passed. Native callback latency is therefore not claimed for this host,
and Linux/Windows native backends remain unexecuted.

## Security and compatibility impact

The authorized root remains startup-only and notify follows no symlinks. Repository
content cannot enable watching or change limits. Events never bypass containment,
ignore, secret or hash validation. The database remains outside the watched root and
runtime network access remains disabled. No MCP tool, Tasks capability or schema
migration is added; modern and legacy clients retain the Phase 11 contract.

## Tests and rollback strategy

Tests cover native backend initialization, explicit polling, debounce, bounded
overflow, lost-event periodic recovery, atomic save, rename/delete/recreate,
case-only rename, Unicode paths, bursts, ignore and Git HEAD changes, all-language
incremental/full equivalence, owner/follower takeover, killed owner recovery,
responsive status and EOF cleanup. Rollback is disabling `--watch`; explicit index
calls and the active generation remain valid without changing the database schema.

## References

- `prompts/12-watch-and-incremental.md`
- `docs/SOURCES.md` S18
- `docs/reports/12-watch-and-incremental.md`
- notify 8.2.0 locked in `Cargo.lock`
