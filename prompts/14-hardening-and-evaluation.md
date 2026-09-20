# Phase 14 — Security, crash recovery, fuzzing and measurements

$ca-phase-driver $ca-security-review $ca-release-validation

## Task and scope
Prove the release candidate against adversarial input, failures, real native process behavior and a recorded performance/accuracy baseline.

## Prerequisites
Phase 13 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/TEST_MATRIX.md`
- `docs/SECURITY_PRIVACY.md`
- `docs/DEPENDENCY_POLICY.md`

## Implementation steps
1. Execute the full test matrix and synthetic acceptance scenarios. Add property
   tests for paths, IDs/ranges, cursors, active-generation joins, budget truncation
   and update/full-rebuild equivalence.
2. Add cargo-fuzz targets for parser/query extraction, path/URI/cursor decoding and
   tool argument handling. Keep any nightly fuzz toolchain separate from production
   stable. Record time/corpus/crashes; a short clean fuzz run is not a security proof.
3. Test native writer termination, interrupted jobs, corrupt DB, migration failures,
   read-only directories, lock contention, watcher overflow, very deep/large source,
   huge graph fanout, malicious FTS input and synthetic credential exclusions.
4. Exercise every attack path through tools AND resources. Audit process/network
   capabilities and confirm the default server never restores dependencies or
   executes repository scripts. Document native grammar/TOCTOU residual risks.
5. Run declaration/reference precision, recall and unresolved-rate evaluation by
   language on hand-labelled fixtures and an authorized, provenance-recorded larger
   corpus. Do not upload private repository contents in reports or CI artifacts.
6. Measure startup (both protocol modes), warm lookups, index throughput, RSS,
   no-op update, single edit, status responsiveness, cancellation and shutdown.
   Record hardware/filesystem/cache state/release hash and all failures vs targets.
7. Run dependency/license/advisory checks, including the actual bundled SQLite
   version. Fix findings or record explicit unresolved blockers; never silently
   suppress the checks or relax assertions to manufacture a release pass.

## Acceptance gates
- Required correctness/security cases pass on supported native targets.
- No active-index corruption or note loss in implemented crash/recovery tests.
- Default source access has no demonstrated root escape or unauthorized execution.
- Budget/cancellation tests observe actual bounded work, not just fast error returns.
- Accuracy and performance reports contain real counts/times and limitations.
- Unavailable targets/checks remain visibly unexecuted; release status reflects that.

## Out of scope
No unsupported safety/performance guarantees, anonymous external corpus upload or automatic publishing.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/14-hardening-and-evaluation.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
