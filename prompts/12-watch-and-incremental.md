# Phase 12 — Watcher, incremental reconciliation and worktrees

$ca-phase-driver $ca-index-storage $ca-rust-architecture

## Task and scope
Keep the index fresh during an explicitly enabled owner session while surviving unreliable filesystem event streams.

## Prerequisites
Phase 11 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/DATA_MODEL.md`
- `docs/ARCHITECTURE.md`
- `docs/TEST_MATRIX.md`

## Implementation steps
1. Add opt-in watcher lifecycle owned by the same write-owner process. Default
   auto_index/watch remain false. Followers do not start competing watchers, and
   shutdown drops the watcher and terminates owned work cleanly.
2. Debounce and coalesce events; treat events as hints, not authoritative operations.
   Re-stat/re-hash paths, handle editor atomic saves/rename pairs and batch changes.
   Bound queues and record overflow instead of dropping changes silently.
3. Add periodic reconciliation and a notify polling fallback when native events are
   unavailable/unreliable. Keep the DB local even when sources are on a mounted path;
   explicitly report degraded source-filesystem guarantees.
4. Detect HEAD/worktree/config/ignore/grammar changes and invalidate the necessary
   scope. Distinct worktrees stay isolated. Git metadata changes must not cause
   obsolete incoming bindings to survive. A gitless root still reconciles normally.
5. Start with changed-file full reparsing plus full graph re-resolution where needed.
   Tree-sitter subtree reuse is optional: implement only with accurate InputEdit
   sequences and equivalence tests. No incorrect old_tree reuse for arbitrary edits.
6. Test real atomic save, rename/delete/recreate, burst writes, watcher overflow,
   branch switch, lost events, case-only renames, Unicode paths and owner crash.
   Compare final incremental facts to a clean full rebuild after each sequence.

## Acceptance gates
- Incremental/full equivalence holds for the edit-sequence corpus across languages.
- Missed/overflowed events eventually reconcile; no permanent stale graph.
- Only the owner watches/writes; followers stay useful and can later acquire ownership.
- Status remains responsive during bursts and pending reconciliation is visible.
- Exit/EOF stops watcher tasks and leaves committed data consistent.
- Native OS watcher tests and measured latency are reported with limitations.

## Out of scope
No mandatory always-running service, auto-modified system watch limits, or claimed perfect filesystem event delivery.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/12-watch-and-incremental.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
