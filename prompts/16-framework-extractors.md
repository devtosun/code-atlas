# Phase 16 — Optional framework structure extractors

$ca-phase-driver $ca-treesitter-language $ca-graph-resolution

## Task and scope
Extend the stable source graph with explicitly labelled framework-specific observations for ASP.NET, Spring, Go Fiber, Express and Flutter-style structures.

## Prerequisites
Phases 00–15 must be complete. This is OPTIONAL and requires an explicit request.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/ARCHITECTURE.md`
- `docs/TEST_MATRIX.md`

## Implementation steps
1. Read the verified release baseline and write an ADR for a FrameworkExtractor
   boundary. Use existing parsed syntax/import evidence; do not run frameworks or
   introduce a package manager/runtime dependency.
2. Add one extractor at a time with independent fixtures: ASP.NET route attributes
   and Minimal API mappings; Spring request-mapping annotations; Go Fiber route
   registrations; recognized Express router forms; Flutter widget-constructor nesting.
3. Prefer import/type/context evidence over method-name-only matching. Same-named
   functions in unrelated packages must be negative fixtures. Dynamic prefixes,
   computed routes, reflection and external config remain uncertain.
4. Store framework facts with extractor/version/source range/hash and confidence
   category. A matching HTTP path across services is a candidate relationship,
   not a compiler-confirmed or runtime-confirmed call chain.
5. Add bounded filtering/projection through existing map/context APIs where possible;
   avoid multiplying tools unnecessarily. Bump extraction fingerprints and migrate
   schema only when needed; preserve compatibility and notes.
6. Measure usefulness and false positives on hand-labelled multi-framework fixtures.
   Document unsupported patterns and keep every baseline language independent.

## Acceptance gates
- Every enabled framework extractor has positive/negative fixtures and provenance.
- Same-name unrelated API calls do not become route facts without context.
- Dynamic/cross-service matches are clearly candidates.
- Enabling/disabling extractors invalidates the correct facts without stale leakage.
- All baseline release correctness/protocol/security gates remain green.

## Out of scope
No runtime tracing, framework execution, universal DI resolution or cross-service certainty.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/16-framework-extractors.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
