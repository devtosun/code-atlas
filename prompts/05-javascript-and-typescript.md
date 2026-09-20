# Phase 05 — JavaScript, JSX, TypeScript and TSX extraction

$ca-phase-driver $ca-treesitter-language $ca-graph-resolution

## Task and scope
Complete JS/TS family extraction, including React-style JSX/TSX, without executing JavaScript configuration.

## Prerequisites
Phase 04 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/DATA_MODEL.md`
- `fixtures/README.md`

## Implementation steps
1. Implement language/dialect routing for js/jsx/mjs/cjs and ts/tsx/mts/cts/d.ts.
   Verify TypeScript and TSX use their distinct grammars. Do not route every file
   into whichever parser tolerates the most syntax without reporting its dialect.
2. Extract declarations/classes/methods, named and arrow-function bindings, scopes,
   ESM imports/exports/re-exports, recognized CommonJS require/module.exports forms,
   call sites/constructors, optional chaining and async functions.
3. TypeScript adds interfaces, type aliases, enums, namespaces, overload declarations,
   generic signatures, type-only imports and decorators where the selected grammar
   supports them. Separate types and values when role evidence exists.
4. Extract JSX/TSX component symbols and reference observations without asserting
   that every JSX tag is an executable direct function call. Dynamic property access,
   dynamic import/require and computed keys remain unresolved/candidate observations.
5. Parse only supported JSON/JSONC manifest/config data through reviewed parsers if
   needed later. Do not execute tsconfig JavaScript, npm scripts or module loaders.
   Path-alias resolution remains explicitly unsupported until the resolver phase.
6. Add fixtures for re-export cycles, alias imports, shadowing, overloaded signatures,
   TSX generics, comments/strings, anonymous callbacks, malformed edits and Unicode.
   Keep snapshots specific enough to reveal false positives and missing scopes.

## Acceptance gates
- JS, JSX, TS and TSX each pass positive and negative fixture gates.
- Declaration/type/import roles and same-name bindings are not conflated.
- Dynamic calls and JSX semantics are not overclaimed.
- No dependency on Node/npm or dependency installation during indexing.
- Query compilation, ranges and cancellation tests pass for all affected dialects.
- Rust/Go and shared parser tests remain green.

## Out of scope
No Vue/Svelte SFC support, JS execution, bundler emulation, type checking or TypeScript language server.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/05-javascript-and-typescript.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
