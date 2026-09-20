# ADR-0002: worker-local parser kernel and honest capability boundary
Status: accepted
Date: 2026-09-19
Phase: 03

## Context and evidence

The server needs seven statically linked Tree-sitter languages plus explicit JSX
and TSX dialect behavior. Tree-sitter parser and query execution are native,
stateful operations; cancellation callbacks are cooperative rather than hard
preemption. Syntax captures also cannot justify compiler-level binding claims.
Phase 00 established exact compatible package versions and grammar ABIs. Phase 03
compiled those versions in the production workspace and executed the original
fixture corpus through the production registry.

## Decision

Keep Tree-sitter and every grammar dependency inside `ca-languages`. A
`ParserWorker` owns its parser and compiled query objects; callers will place these
workers behind a bounded CPU queue when indexing is introduced. Bound source bytes,
native progress callbacks, syntax traversal, diagnostics, in-progress query matches
and returned captures. Reset a parser after any callback-driven interruption.

Return only owned syntax facts. Ranges address original valid UTF-8 bytes using a
zero-based inclusive start and exclusive end. Human lines are one-based and byte
columns zero-based. Preserve BOM and CRLF bytes; reject invalid UTF-8 before native
parsing. Attach source, grammar and query fingerprints to every result.

Phase 03 declaration queries are reviewed kernel anchors, not complete adapters.
Providers advertise parser readiness separately from extractor/category readiness;
all complete extractor flags remain false until phases 04–07.

## Alternatives considered

- A parser shared behind a mutex was rejected because it obscures worker ownership
  and risks holding synchronization across scheduling boundaries.
- Aborting a blocking task was rejected as cancellation proof because it does not
  stop an already-running native parse.
- Regex fallback and generic “all identifiers” queries were rejected because they
  create false semantic claims and include comments/strings or non-reference roles.
- Stripping BOM or normalizing newlines was rejected because stored ranges would no
  longer address the hashed source bytes.

## Trade-offs and failure modes

Cooperative callbacks do not provide a hard wall-clock deadline; a native grammar
bug can still hang or crash the process. Source and work bounds reduce exposure but
do not create a memory-safety sandbox. The initial declaration anchors are useful
for contract tests but intentionally incomplete. A grammar/query change invalidates
stored versions and reviewed fixture snapshots.

## Security and compatibility impact

Only statically linked, pinned grammar packages and embedded trusted queries are
loaded. Repository content remains data and is never executed. Native grammar code
is trusted dependency code. JSX uses the JavaScript grammar through an explicit
provider; TSX uses the separate TSX grammar value.

## Tests and rollback strategy

`xtask grammar-check` loads all providers and compiles capture queries.
`xtask fixtures` verifies 24 original files against hand labels and a reviewed
golden. Unit tests cover invalid queries, UTF-8/BOM/CRLF positions, malformed and
empty input, limits, cancellation/progress interruption, parser reset and owned
deterministic results. Rollback removes the production grammar dependencies and
restores Phase 02's dormant `ca-languages`; no stored parser index exists yet.

## References

- `docs/reports/03-parser-kernel.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/GRAMMAR_MATRIX.md`
- `fixtures/parser-kernel.golden`
