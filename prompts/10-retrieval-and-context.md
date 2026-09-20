# Phase 10 — Search, graph queries and bounded context assembly

$ca-phase-driver $ca-graph-resolution $ca-security-review

## Task and scope
Implement useful engine queries with compact, deterministic, source-grounded results ready for MCP exposure.

## Prerequisites
Phase 09 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/MCP_CONTRACT.md`
- `docs/DATA_MODEL.md`
- `docs/TEST_MATRIX.md`

## Implementation steps
1. Implement exact/name/prefix search before FTS5 ranking. Add identifier token
   splitting for camelCase/snake_case and preserve exact spellings. Escape literal
   FTS syntax separately from parameterized SQL; bound query length and result count.
2. Implement get_symbol, file outline, references, caller/callee traversal, structural
   repo map and impact analysis with graph depth/node/edge/deadline limits. Treat
   “no resolved caller” as missing evidence, not proof of unused/dead code.
3. Build generation-aware deterministic pagination with query-bound cursors. Reject
   malformed/stale/cross-query cursors and never mix generations in one response.
4. Implement authorized read_code and bounded evidence snippets. Verify indexed hash
   against actual source at read time; stale/deleted/newly excluded paths return
   explicit errors, not unrelated bytes at old offsets.
5. Implement build_context as deterministic retrieval/selection: rank declarations,
   relevant neighbors and snippets; deduplicate overlap; include provenance,
   coverage, uncertainty and truncation. No embedded LLM or API request.
6. Enforce final serialized result size including escaping/metadata/TextContent
   fallback. A token estimate must name its approximation method; byte cap is the
   authoritative v1 guarantee. Do not trim serialized JSON blindly.
7. Test punctuation, empty queries, Unicode, same names, search ranking direction,
   duplicate snippets, stale hashes, cycles, huge fanout and response caps.

## Acceptance gates
- Exact/qualified matches rank ahead of weaker lexical matches on labelled cases.
- All reads use one generation and root policy, including snippets and outlines.
- Graph and context outputs disclose candidates/unresolved/parse coverage.
- Pagination is deterministic and fails safely after invalidation/GC.
- Serialized responses always obey configured caps and remain valid JSON.
- No query unexpectedly indexes, executes code, or writes user memory.

## Out of scope
No embeddings, model runtime, natural-language generation service or external vector store.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/10-retrieval-and-context.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
