# ADR-0009: generation-bound retrieval and exact response budgets
Status: accepted
Date: 2026-09-19
Phase: 10

## Context and evidence

Phase 09 produced immutable syntax observations and a generation-scoped evidence
graph, but had no unified retrieval policy. Phase 10 needs ranked symbol discovery,
navigation, fresh source excerpts and context selection without allowing SQLite,
transport handlers or callers to bypass generation, certainty or output limits.
FTS input is its own injection boundary, cursors can accidentally cross snapshots,
and an apparent payload limit can be exceeded after JSON escaping and TextContent
fallback duplication.

Tests exercise empty, oversized, punctuation-shaped and Unicode queries; same-name
symbols across files; cross-query, stale and foreign cursors; cycles, fanout and
candidate opt-in; changed, deleted and excluded source; overlapping evidence; and
escaped content whose serialized representation is larger than its source string.

## Decision

Add `ca-engine::retrieval` as the policy-owning use-case layer behind a typed
`RetrievalStore` port. A CLI adapter maps one pinned SQLite `ReadSnapshot` into that
port. Keep the MCP surface unchanged until Phase 11.

Rank qualified exact, exact, case-folded exact, case-sensitive prefix and folded
prefix matches before FTS5. Split identifier search documents at camelCase, acronym,
snake and punctuation boundaries. Construct only quoted alphanumeric literal-token
FTS expressions and pass every value through parameterized SQL. Bump the index
configuration fingerprint so reused immutable versions refresh derived documents;
do not migrate schema version 4 for a derivation-policy change.

Encode cursor version, operation, repository ID, generation ID, normalized
query/filter fingerprint and complete deterministic sort tuple. Reject malformed,
foreign, stale and cross-query cursors rather than continuing against a different
snapshot.

Return certainty, candidate counts, reasons, limitations and coverage on graph
results. Traverse with cycle guards plus caller-supplied limits bounded by policy for
depth, nodes, edges and cooperative deadline. An empty incoming-call result carries
a warning that it is not dead-code proof.

For source-bearing results, resolve only normalized relative paths through the
existing authorized, ignore-aware, no-follow `SourceReader`. Compare the live BLAKE3
hash with the selected immutable file version. Distinguish changed, deleted, excluded
and non-indexed source.

Assemble context deterministically from ranked symbols, non-overlapping source
evidence and bounded relations. Report token count only as
`ceil(UTF-8 JSON bytes / 4)`. The authoritative response limit is the exact UTF-8
length of a serialized projection containing both structured content and its JSON
TextContent fallback. Reduce whole items or lines and reserialize; never truncate
encoded JSON bytes.

## Alternatives considered

- Accept raw FTS syntax. Rejected because callers could change query semantics and
  because quoting SQL parameters alone does not make FTS syntax literal.
- Put ranking and cursor policy entirely in SQLite. Rejected because it couples
  application contracts to infrastructure and makes alternate stores inconsistent.
- Use offsets or active-generation-only cursors. Rejected because concurrent
  activation can silently reorder or mix pages.
- Return indexed source bytes. Rejected because old offsets can disclose stale or
  newly excluded content after the worktree changes.
- Estimate result size from source strings or token counts. Rejected because JSON
  escaping and duplicated fallback content can exceed the actual protocol budget.
- Add embeddings or an LLM reranker. Rejected as outside the local deterministic v1
  scope and unnecessary for Phase 10 acceptance.

## Trade-offs and failure modes

Search candidates are deliberately bounded, so extremely deep result sets may need
future indexed keyset-query optimization. Token estimates are heuristic and named as
such. Context deduplicates overlapping ranges by deterministic rank rather than
trying to synthesize a larger semantic unit. Live source changes cause explicit
errors or warnings instead of serving possibly useful stale text.

Phase 10 has deterministic correctness evidence on small fixtures, not warm-query
p95 data for 100k declarations or peak-memory measurements. Linux and Windows native
behavior remains unexecuted on the available host.

## Security and compatibility impact

No new third-party dependency, schema version, runtime network access, code execution
or mandatory process is introduced. SQLite reads are generation-scoped and
parameterized. Source authorization, secret/generated exclusions and link refusal are
reapplied at read time. Cursor fields are opaque application data, not authorization.
The server still advertises only `repository_status`, so no untested wire contract is
introduced in Phase 10.

## Tests and rollback strategy

Engine tests cover ranking, tokenization, cursor binding, cycles/fanout, candidate
disclosure, source freshness, overlap deduplication and exact serialized byte caps.
Storage tests execute literal-safe FTS and rich generation-scoped relation queries
against bundled SQLite. Workspace tests verify that the MCP advertisement remains
status-only.

Rollback keeps schema version 4 readable. Reverting the search-document fingerprint
can reuse older derived documents but must not silently claim token-split search
coverage. Databases or user memories must never be deleted as a rollback mechanism.

## References

- `prompts/10-retrieval-and-context.md`
- `docs/ARCHITECTURE.md`
- `docs/DATA_MODEL.md`
- `docs/MCP_CONTRACT.md`
- `docs/SECURITY_PRIVACY.md`
- `docs/reports/10-retrieval-and-context.md`
