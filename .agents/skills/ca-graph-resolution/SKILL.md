---
name: ca-graph-resolution
description: "Implement lexical/import binding, graph traversal, ranking or context packing for CodeAtlas; use when relation certainty, evidence or output budgets matter."
---

# Graph and Retrieval


## Workflow
Read docs/LANGUAGE_SUPPORT.md, DATA_MODEL.md and MCP_CONTRACT.md. Separate syntax
observations from binding assertions. Resolve lexical scope and supported imports
before candidate search. Preserve ambiguous overloads, dynamic receivers, external
imports, interfaces/traits, dependency injection and unsupported module rules.

Every edge records source occurrence/range/hash, destination or candidate set,
resolution label, rule/version and limitations. Candidate edges are opt-in to
trace_calls and clearly marked in impact results. Cross-language route strings
are hints, not semantic call edges. An absence of resolved edges is not proof that
a symbol is unused or safe to delete.

Use exact identifiers/path and normalized identifier tokens before FTS BM25.
Escape FTS literal syntax and parameterize SQL. A smaller native FTS5 BM25 value
ranks better; test ranking rather than assuming score direction. Scope all joins
to one generation. Deterministic tie-breaking and cursors avoid reordered pages.

Graph traversals have visited sets, depth/node/edge/deadline bounds. Context packing
selects evidence deterministically, de-duplicates overlapping spans and accounts for
JSON escaping/wrappers/fallback text. Do not imply tokenizer-independent exact token
counts. Stale file hashes prevent returning old ranges as fresh source snippets.

## Tests
Shadowing, unrelated same-name methods, ambiguous imports, overloads, re-export
cycles, recursive calls, external targets, deleted declarations, search punctuation,
Turkish/Unicode identifiers, output cap and stale cursors. Keep precision/recall
and unresolved rate separate; never create expected data from your own algorithm.
