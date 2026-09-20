# Phase 03 — Parser registry, capture contracts and fixtures

$ca-phase-driver $ca-treesitter-language $ca-rust-architecture

## Task and scope
Build shared parsing infrastructure and an executable language-contract test harness without pretending all extraction adapters are already complete.

## Prerequisites
Phase 02 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/GRAMMAR_MATRIX.md`
- `docs/SECURITY_PRIVACY.md`
- `fixtures/README.md`

## Implementation steps
1. Promote verified grammar providers to ca-languages. Implement explicit extension/
   dialect detection, separate TSX provider, a registry and owned ExtractionResult
   types with declarations/scopes/imports/references/call sites/diagnostics.
2. Keep parser instances worker-local and queries embedded. Establish documented
   capture conventions and compile/query tests. Inspect actual node-types before
   authoring .scm files; unsupported extraction modes return a clear diagnostic.
3. Implement bounded parsing/capture execution, cooperative cancellation/progress
   hooks and parser reset after cancellation. Include parse errors/MISSING nodes in
   coverage. Do not claim hard native timeouts if the API only permits cooperation.
4. Establish exact UTF-8 byte/exclusive-end ranges and explicit line mapping. Test
   Unicode, CRLF, BOM policy, empty/incomplete source and invalid encoding rejection.
5. Build a fixture harness using the provided original seed files and manually
   labelled expectations. Add support for positive/negative assertions and golden
   snapshots without automatic approval. Store grammar/query hashes in output.
6. Implement xtask `grammar-check` and `fixtures` with meaningful failure codes.
   Each language's extraction capability remains pending until its dedicated phase
   passes all tests; registry presence alone does not flip support to complete.

## Acceptance gates
- All grammar/dialect providers load through the production registry.
- The same fixture bytes produce deterministic ranges and owned observations.
- Invalid .scm capture patterns fail with language/query context.
- Limits/cancellation/reset and malformed source tests run without hangs.
- The fixture harness fails on missing declarations or false positive references.
- Capability reports accurately distinguish parser ready from extractor ready.

## Out of scope
No guessed all-language extraction, regex parser substitute, dynamic grammar loading or semantic certainty claims.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/03-parser-kernel.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
