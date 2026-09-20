# Phase 08 — Persistent indexing pipeline and cancellable jobs

$ca-phase-driver $ca-index-storage $ca-rust-architecture

## Task and scope
Connect safe scanning, seven-language extraction and SQLite into a real persistent index exposed through CLI and engine use cases.

## Prerequisites
Phase 07 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/DATA_MODEL.md`
- `docs/MCP_CONTRACT.md`
- `docs/SECURITY_PRIVACY.md`

## Implementation steps
1. Implement bounded scan -> read/hash -> parse -> persist pipeline with explicit
   job states/progress. Keep protocol runtime independent; use bounded source queues
   and worker limits. Do not accumulate every file's AST/source in memory.
2. Store immutable versions, owned syntax observations, search rows and generation
   membership. Reuse a file only when content and config/grammar/query/extractor
   fingerprints match. Do not rely solely on mtime/size.
3. Implement full and changed-file indexing with atomic activation. At this phase
   generation resolution coverage is honestly syntax-only until phase 09 adds the
   resolver. Do not emit dummy successful semantic edges.
4. Complete traversal before inferring deletions. Handle unreadable directories,
   file edits during reads, invalid encoding, oversized inputs and per-file parse
   errors according to the persistence contract. Persist coverage and warnings.
5. Implement durable jobs, request-key deduplication scoped to root/operation,
   cooperative cancellation, interrupted-owner recovery and abandoned-generation GC.
   Cancellation checks reach parser/query work, not merely the async response future.
6. Add `codeatlas index/status/doctor` engine-backed CLI operations and job tests.
   On a write-owner conflict return a bounded actionable error; never start a daemon.
7. Test no-op reindex, modified/deleted/renamed files, identical mtime with changed
   content, concurrent readers, killed writer, failed traversal and memory bounds.

## Acceptance gates
- CLI indexes all supported seed languages and persists facts across restart.
- No-op indexing reuses unchanged file versions deterministically.
- Failed/cancelled generation never displaces the previous active one.
- FTS/facts/deletions are consistent in the activated generation.
- Busy owner, parser limits and scan failures are visible, not hidden success.
- Job progress reflects actual counts; cancellation is observed in workers.

## Out of scope
No watcher yet, no public fake tools, no advanced resolution, no mandatory model/DB service.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/08-index-jobs.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
