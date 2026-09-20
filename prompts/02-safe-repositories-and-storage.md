# Phase 02 — Authorized paths, worktrees and SQLite generations

$ca-phase-driver $ca-index-storage $ca-security-review

## Task and scope
Implement safe source access and durable storage foundations before ingesting repository content.

## Prerequisites
Phase 01 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/DATA_MODEL.md`
- `docs/SECURITY_PRIVACY.md`
- `docs/ARCHITECTURE.md`

## Implementation steps
1. Implement trusted root authorization, component-aware containment and a
   SourceReader/FileScanner. Respect ignore files, hard exclusions, size limits,
   binary/invalid-UTF8 diagnostics and non-following symlink policy. Prevent races
   where practical using safe handles; document remaining platform limitations.
2. Derive worktree-specific identities, including Git .git files and gitless roots.
   Use narrowly configured gix for read-only metadata; never spawn Git hooks or
   evaluate build manifests. Resolve local application-data directories outside source.
3. Design and implement versioned SQLite migrations for generations, immutable file
   versions/facts, search documents, jobs and memory scaffolding as needed. Enforce
   foreign keys/uniqueness; probe SQLite safety floor/FTS5 on open.
4. Implement nonblocking OS write-owner acquisition, a dedicated writer thread,
   short transactions and bounded busy handling. Followers query safely or return
   WRITER_BUSY for mutations. DB initialization/migrations never block discovery.
5. Add active-generation read snapshots and atomic activation primitives with test
   facts, not fabricated extraction. Implement a WAL-safe backup mechanism and
   refusal of unknown future schemas. Keep notes durable through upgrades.
6. Test interrupted staging, active readers, OS lock contention, reopen recovery,
   permissions, corrupt files, disk errors where injectable, symlinks/junctions,
   root prefix tricks, Windows paths and two worktrees with the same remote.

## Acceptance gates
- Unauthorized/out-of-root files are never read through scanner or reader APIs.
- Databases are local and distinct for distinct worktrees; gitless roots work.
- A reader sees one committed generation; failed activation leaves the old one.
- No writer lock/migration can hang MCP discovery; followers return explicit errors.
- SQLite/FTS5 version checks and migration/backup tests run with the real engine.
- Native path/lock checks run on available hosts; unavailable platforms are recorded.

## Out of scope
No language extraction, code execution, directory-wide auto-index at launch or destructive database repair.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/02-safe-repositories-and-storage.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
