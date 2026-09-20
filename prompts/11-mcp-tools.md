# Phase 11 — Production MCP tool surface and end-to-end contracts

$ca-phase-driver $ca-mcp-contract $ca-security-review

## Task and scope
Expose the thirteen implemented status/index/job/source/search/graph/context operations as real typed MCP tools. Memory tools arrive in phase 13.

## Prerequisites
Phase 10 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/MCP_CONTRACT.md`
- `docs/SECURITY_PRIVACY.md`
- `docs/TEST_MATRIX.md`

## Implementation steps
1. Add thin rmcp handlers for repository_status, index_repository, job_status,
   cancel_job, search_symbols, get_symbol, find_references, trace_calls,
   get_file_outline, read_code, get_repo_map, analyze_impact and build_context.
   Delegate policy/business work to engine use cases. No duplicate SQL in handlers.
2. Generate/validate input/output schemas and set accurate annotations. Root is
   startup-authorized, not a model-supplied arbitrary path. Reject cross-root IDs,
   invalid enums/ranges, excessive limits and unknown fields where appropriate.
3. Map application errors to actionable isError results with stable app codes;
   preserve SDK protocol errors. Include structured payload and compatible text
   representation while enforcing the whole-frame size budget.
4. Ensure long indexing returns job_id quickly and status remains responsive.
   Test idempotency, cancellation vs commit races and process shutdown. Do not claim
   that application jobs automatically implement standardized MCP Tasks.
5. Run independent subprocess wire tests in modern and legacy modes for every tool,
   including malformed/unsupported versions, concurrency, output-schema validation,
   stdout contamination, locked DB and no index. Exercise full CLI binary, not just
   mocked handler calls.
6. Prepare a manual Codex smoke recipe without changing its config automatically.
   Record whether actual installed Codex integration was run and its version; do
   not equate SDK-only tests with real-client verification.

## Acceptance gates
- Thirteen real tools work end to end with persisted fixture data.
- Both protocol eras have valid schema/result/lifecycle behavior.
- Discovery/tools-list/status do not wait for indexing or writer-lock contention.
- Source access and output caps cannot be bypassed through tool parameters.
- Every application failure is actionable; no panic or fake empty successful result.
- EOF/cancellation tests leave no orphan worker process or incomplete active index.

## Out of scope
No unimplemented memory tools, auto-install, HTTP endpoint, remote execution or custom JSON-RPC implementation.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/11-mcp-tools.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
