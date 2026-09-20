# Phase 09 — Scope/import resolution and evidence-backed graph

$ca-phase-driver $ca-graph-resolution $ca-index-storage

## Task and scope
Build a useful graph while making ambiguity a first-class result rather than inventing compiler-level precision.

## Prerequisites
Phase 08 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/DATA_MODEL.md`
- `docs/TEST_MATRIX.md`

## Implementation steps
1. Implement lexical scope binding and supported same-file/module/import rules
   per language. Record rule/version, occurrence range, target declaration, candidate
   count and resolution label. Keep unresolved observations queryable.
2. Add bounded import/re-export traversal, alias handling and cycle detection.
   Parse supported manifests as data only. Unsupported module/path/build rules produce
   explicit diagnostics; do not assume file basename uniquely identifies a module.
3. Preserve overload sets and receiver uncertainty. Do not turn interface/trait/DI/
   virtual calls into implementation edges from a method-name match. External
   imports are external nodes/observations, not fabricated local declarations.
4. Persist generation-scoped containment/import/reference/call edges. Require all
   local edge endpoints to belong to the generation's file-version membership.
   Initially re-resolve the entire candidate generation after file changes; optimize
   invalidation only after equivalence tests prove incoming/outgoing consistency.
5. Add bounded graph read ports for incoming/outgoing neighbors and call sites.
   Candidates are excluded by default and included only with an explicit flag.
6. Build an independently labelled multi-file evaluation corpus per language:
   aliases, shadowing, imports with duplicates, overloads, dynamic targets, recursive
   cycles, file deletion/rename and changed exports. Report precision/recall/unresolved
   rate separately and include sample sizes, not an invented single success score.

## Acceptance gates
- Hand-labelled deterministic lexical/import cases resolve to correct declarations.
- Ambiguous/dynamic/interface cases remain candidates or unresolved as expected.
- Edges reference only the selected generation; deletes leave no stale targets.
- Candidate opt-in and cycle limits are enforced in graph APIs.
- Full and changed-file indexing produce equivalent graph facts on the corpus.
- Per-language evaluation reports distinguish syntax coverage from binding accuracy.

## Out of scope
No universal type inference, compiler invocation, cross-language semantic claims or arbitrary graph query language.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/09-resolution-and-graph.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
