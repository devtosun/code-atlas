# ADR-0003: syntax-scoped Rust and Go extraction
Status: accepted
Date: 2026-09-19
Phase: 04

## Context and evidence

The Phase 03 parser kernel deliberately exposed only declaration anchors. Rust and
Go need useful declarations, scopes, imports, references and call sites without
claiming compiler or language-server semantics. Both languages include constructs
whose meaning depends on unavailable evidence: Rust macro expansion and cfg, and Go
package loading, interface satisfaction and build constraints.

The Phase 04 implementation inspected the pinned grammar node types and parse trees,
compiled separate symbols/imports/references/calls query assets, and executed
multi-file positive, negative, invalid-edit and Unicode fixtures.

## Decision

Use Tree-sitter queries to select reviewed syntax nodes and language-specific Rust
and Go adapters to normalize them into one owned observation model. Each observation
has a category/kind, exact name range, containing syntax range, deterministic ID,
owning lexical scope ID and optional container, signature, receiver type, alias,
target, attributes and limitations.

IDs hash language, repository-relative path, category, kind, spelling and a
named-node structural path. They therefore survive pure line insertion and other
byte-offset changes that preserve tree structure. They are not promised stable when
named sibling structure changes.

Rust grouped use trees are expanded into leaf observations. Go package clauses and
imports record their explicit or syntax-derived aliases. Captured references bind
only to containing lexical scopes; this phase does not select declaration targets.
Every call is `unresolved` with no target ID. Rust macro calls are retained but token
trees are not expanded. Rust cfg/cfg_attr and Go build tags are condition
observations with explicit unknown-active-configuration limitations.

Rust and Go advertise extractor and extraction-category readiness, but keep name
resolution readiness false. Other language providers remain at the parser-kernel
boundary.

## Alternatives considered

- Byte-offset-based declaration IDs were rejected because inserting lines would
  churn identity without changing a declaration.
- Project-wide same-name matching was rejected because receiver methods,
  shadowing, traits and interfaces make it unsound.
- Treating macro tokens as ordinary Rust references was rejected because expansion
  and hygiene are unavailable.
- Running Cargo, Go tooling, build scripts or language servers was rejected because
  it exceeds the local read-only syntax boundary and Phase 04 scope.

## Trade-offs and failure modes

Structural IDs can change after named sibling edits. Syntax references include type
and selector components without binding evidence. Go default import aliases are
derived from the final path segment but do not prove the imported package clause.
Malformed trees yield the observations the grammar can still recover plus explicit
syntax diagnostics. Trait/interface calls, generic instantiations and
constructor-like syntax remain unresolved.

## Security and compatibility impact

The implementation reads provided source bytes only. It performs no package
restore, build, macro expansion, hook, external process or network operation. Query
and extractor fingerprints invalidate cached facts after adapter/query changes.
Existing source, syntax-node, capture, query-match, diagnostics and cooperative
progress bounds remain in force.

## Tests and rollback strategy

`xtask rust-go-fixtures` verifies eight focused cases against independent labels and
a reviewed exact category hash golden. Unit tests cover stable IDs under line
insertion, distinct nested bindings and receiver methods, unresolved calls,
determinism, limits, cancellation and parser reuse. The 24-file Phase 03 golden runs
unchanged for all providers except deliberately reviewed Rust/Go query output.

Rollback restores Rust/Go parser-kernel capabilities and the Phase 03 query assets;
no stored parser index or public MCP extraction tool exists yet.

## References

- `docs/reports/04-rust-and-go.md`
- `docs/LANGUAGE_SUPPORT.md`
- `fixtures/rust-go-expectations.json`
- `fixtures/rust-go.golden`
