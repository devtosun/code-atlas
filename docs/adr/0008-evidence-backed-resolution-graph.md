# ADR-0008: evidence-backed generation-scoped resolution graph
Status: accepted
Date: 2026-09-19
Phase: 09

## Context and evidence

Phase 08 persisted immutable syntax observations but intentionally created no
cross-file edges. Phase 09 needs useful navigation without claiming compiler-level
type resolution. A name match alone is insufficient for overloads, shadowing,
receiver methods, interface/trait/virtual dispatch, dynamic module targets and
language-specific build configuration. Incremental parsing also creates a consistency
risk if new edges can point at file versions outside the candidate generation.

The implementation was evaluated against 35 independently authored sites, five per
required language, covering lexical shadowing, aliases, imports, overloads, receiver
uncertainty, external modules, dynamic targets and a recursive re-export cycle. A
separate mutation corpus deletes and renames files and changes an export/import; its
incremental graph facts equal a clean full index of the final tree.

## Decision

Add a pure `ca-engine::resolution` feature with typed inputs and outputs. It applies
deterministic lexical scope chains first, then narrow supported module/import rules.
Every edge retains its source observation and range, rule and resolver versions,
resolution label, candidate count, evidence and applied limits. Overload and receiver
uncertainty produce candidate edges; dynamic or unsupported targets stay unresolved.
Static external imports become explicit external nodes rather than fabricated local
symbols.

Perform full-generation re-resolution after every candidate parse, including
incremental jobs. Persist the result in schema version 4 before activation. Composite
foreign keys plus writer validation require the exact source and local target file
versions to belong to that generation. Activation refuses generations with no
recorded resolver version.

Support only bounded, inert manifest data needed by implemented rules: the root
`go.mod` module declaration and root `pubspec.yaml` package name. Never invoke a
compiler, SDK, package manager, build script, module loader or executable repository
configuration.

Expose engine/storage read ports for bounded incoming/outgoing neighbors and call
sites. Traversal uses explicit depth, node and edge caps and cycle guards. Candidate
edges are excluded by default and require an explicit flag.

## Alternatives considered

- Match every call by method name. Rejected because it fabricates certainty across
  receivers, overloads and dynamic dispatch.
- Resolve only changed files and their apparent imports. Deferred because complete
  invalidation is language-specific and the first correctness requirement is that
  incremental and clean full graphs converge.
- Invoke compilers or language servers. Rejected for v1 because it adds runtime
  dependencies, code/config execution risk and inconsistent availability.
- Store targets directly on immutable syntax observations. Rejected because binding
  depends on the selected repository generation and resolver version.
- Expose arbitrary SQL or graph query syntax. Rejected because it bypasses resource,
  authorization and output bounds.

## Trade-offs and failure modes

Full graph re-resolution is simpler and consistent but can become the dominant cost
on large repositories; Phase 14 must measure it before dependent invalidation is
designed. Narrow module rules leave package exports, path aliases, Dart part/package
configuration, active cfg/build/preprocessor selection and compiler dispatch
unresolved. Small fixture precision/recall is contract evidence, not real-world
accuracy. If resolver limits are reached, diagnostics and incomplete coverage are
preserved rather than silently dropping uncertainty.

## Security and compatibility impact

No new third-party dependency or runtime network access is added. Manifest reads use
the existing authorized, no-follow repository reader and treat content as UTF-8 data.
All SQL remains parameterized and behind bounded typed ports. Existing syntax tables
and persistent memories survive the v3-to-v4 migration. The MCP advertised surface
remains status-only, so Phase 09 changes index contents without promising public graph
tools or wire compatibility that has not yet been tested.

## Tests and rollback strategy

Unit tests cover shadowing, overload candidates, dynamic targets, aliases,
re-exports, cycles, manifest parsing and bounded graph traversal. Storage tests cover
candidate opt-in, call-site filtering, generation membership, stale-target rejection
and deletion activation. CLI tests compare all 35 labels and incremental/full graph
facts. Rolling back code requires a reader compatible with schema version 4 or an
explicit supported migration; never delete or recreate a user database implicitly.

## References

- `prompts/09-resolution-and-graph.md`
- `docs/DATA_MODEL.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/reports/09-resolution-and-graph.md`
- `docs/reports/artifacts/09-resolution-evaluation.json`
