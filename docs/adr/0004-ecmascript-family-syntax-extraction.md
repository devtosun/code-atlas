# ADR-0004: role-aware ECMAScript-family syntax extraction
Status: accepted
Date: 2026-09-19
Phase: 05

## Context and evidence

JavaScript, JSX, TypeScript and TSX share substantial syntax but do not share one
interchangeable grammar or one semantic namespace. TypeScript and TSX use distinct
pinned grammar values. Type and value declarations can legitimately share a name,
ESM re-exports can form cycles, CommonJS and module specifiers can be dynamic, and
JSX tag syntax does not establish a runtime function call.

The implementation inspected parse trees and node-type metadata from the pinned
`tree-sitter-javascript` 0.25.0 and `tree-sitter-typescript` 0.23.2 bindings before
compiling separate symbols/imports/references/calls query assets for each dialect.
Focused fixtures exercise positive, negative, malformed and Unicode cases.

## Decision

Route `.js`/`.mjs`/`.cjs`, `.jsx`, `.ts`/`.mts`/`.cts`/`.d.ts`, and `.tsx` to their
explicit registry dialects. JavaScript and JSX share the JavaScript grammar while
retaining different query fingerprints; TypeScript and TSX use distinct grammar
values and grammar fingerprints.

Normalize the four dialects through one role-aware ECMAScript-family adapter. It
emits declarations, lexical scopes, imports/exports, references and unresolved
calls with the shared owned observation model and structural IDs from ADR-0003.
TypeScript declarations attach `role:type`, `role:value`, or `role:namespace`
attributes where syntax supports that distinction. Same-name type/value declarations
remain separate records with separate IDs. Overload signatures remain distinct
declarations and are not linked to an implementation in this phase.

Record ESM imports, exports and re-exports plus reviewed static/dynamic `require`,
dynamic `import`, `module.exports` and `exports.name` syntax. Re-exports carry a
cycle-guard requirement and all module observations state that module/path-alias
resolution was not performed. Computed targets and dynamic specifiers retain an
explicit unresolved limitation.

Treat JSX opening and self-closing tag names as reference observations. Mark them
as intrinsic/component syntax and attach `jsx_tag_not_a_call_edge`; do not add call
sites merely because JSX syntax names a component. Every actual call/constructor
site remains `unresolved` with no target ID.

## Alternatives considered

- Routing TypeScript and TSX through whichever grammar accepts the input was
  rejected because it erases dialect evidence and changes parse ambiguity.
- Treating all identifiers as one namespace was rejected because TypeScript permits
  distinct type and value roles.
- Resolving imports from `tsconfig`, package manifests or runtime loaders was
  rejected because Phase 05 has no reviewed configuration resolver and must not
  execute JavaScript or package tooling.
- Converting JSX tag names into direct calls was rejected because framework
  transforms, intrinsic elements, factories and runtime configuration are absent.

## Trade-offs and failure modes

Syntax-only role evidence does not provide type checking, overload selection,
package resolution, CommonJS/ESM interop semantics or runtime dispatch. Per-specifier
type modifiers and reviewed common import shapes are retained, but bundler aliases,
conditional exports and `tsconfig` path mappings remain unresolved. Computed member
names and dynamic module expressions can be observed without a target. Decorator
syntax is recorded without evaluating decorator behavior.

Malformed inputs receive a synthetic root file scope when the query cannot capture
the error-root as a normal program node, preserving scope ownership for recovered
observations while still returning syntax diagnostics.

## Security and compatibility impact

The adapter consumes only supplied UTF-8 bytes and statically linked grammars. It
does not invoke Node, npm, package scripts, config files, module loaders, bundlers or
the TypeScript language service. Existing source, traversal, capture, query-match,
diagnostic and cooperative cancellation bounds remain in force. Query/extractor
fingerprints invalidate cached facts when assets or normalization change.

## Tests and rollback strategy

`xtask js-ts-fixtures` checks eleven focused cases against independent semantic
expectations and a reviewed exact category-hash golden. Unit tests verify explicit
routing, distinct TS/TSX grammar fingerprints, type/value separation, JSX non-call
semantics, dynamic uncertainty and cancellation for all four dialects. The original
24 parser fixtures and eight Rust/Go fixtures remain regression gates.

Rollback restores the Phase 03 declaration-anchor queries and parser-kernel
capabilities for these four dialect targets. No parser-derived persistent index or
public MCP extraction tool exists yet.

## References

- `docs/reports/05-javascript-and-typescript.md`
- `docs/LANGUAGE_SUPPORT.md`
- `fixtures/js-ts-expectations.json`
- `fixtures/js-ts.golden`
