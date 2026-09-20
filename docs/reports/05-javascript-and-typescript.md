# Phase 05 report — JavaScript, JSX, TypeScript and TSX extraction

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git worktree (`git status` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 / `aarch64-apple-darwin`; Rust 1.98.1 (`48a229ceaefd4985c50990b14116b6d856af0985c`, LLVM 22.1.8)

## Scope implemented

- Added separately embedded and compiled symbols, imports, references and calls
  queries for JavaScript, JSX, TypeScript and TSX after inspecting the pinned
  grammar node types and representative parse trees.
- Preserved explicit routing for `.js`/`.mjs`/`.cjs`, `.jsx`,
  `.ts`/`.mts`/`.cts`/`.d.ts`, and `.tsx`. TypeScript and TSX use distinct grammar
  values and fingerprints; JSX remains a separately reported dialect over the
  JavaScript grammar.
- Added an ECMAScript-family source adapter for declarations, lexical scopes,
  classes/methods/constructors, named/arrow-function bindings, parameters, async
  and generic signatures, ESM imports/exports/re-exports, reviewed CommonJS forms,
  references, optional calls, calls and constructors.
- TypeScript extraction adds interfaces, type aliases, enums, namespaces, overload
  declarations, type-only imports and decorator syntax. Explicit type/value/
  namespace attributes keep same-name declarations separate.
- JSX/TSX opening and self-closing tag names are reference observations with an
  explicit non-call limitation. Computed property calls and dynamic `import`/
  `require` expressions remain unresolved with reason-bearing limitations.
- Every call is `unresolved` with no target ID. Re-exports carry a cycle-guard
  requirement. Module and path-alias resolution are explicitly not performed.
- Malformed recovery synthesizes a file scope when an error-root prevents the
  normal program capture, so recovered declarations remain scoped while diagnostics
  continue to report the syntax error.
- No JavaScript, config file, Node/npm command, package script, module loader,
  bundler, type checker or TypeScript language service is invoked.
- Added eleven focused cases with independent semantic expectations and a reviewed
  exact category-hash golden. The 24 parser fixtures and eight Rust/Go extraction
  cases remain regression gates.

## Files changed

- Extraction runtime: `crates/ca-languages/src/{adapters,lib,registry,worker}.rs`
- JS/TS queries: `crates/ca-languages/queries/{javascript,jsx,typescript,tsx}/{symbols,imports,references,calls}.scm`
- Focused corpus: JavaScript cycle/module/JSX files and TypeScript advanced/TSX/
  declaration/routing files under `fixtures/{javascript,typescript}/`
- Expectations/goldens: `fixtures/{js-ts-expectations.json,js-ts.golden,parser-kernel.golden}`
- Harness: `xtask/src/main.rs`
- State/decisions: `config/dependency-lock.json`,
  `docs/adr/0004-ecmascript-family-syntax-extraction.md`,
  `docs/{ARCHITECTURE,DATA_MODEL,GRAMMAR_MATRIX,LANGUAGE_SUPPORT,PROJECT_STATE,TEST_MATRIX}.md`,
  `fixtures/README.md`

## Decisions and evidence

ADR-0004 records explicit dialect routing, role-aware TypeScript declarations,
reviewed ESM/CommonJS normalization, JSX-as-reference treatment and the prohibition
on config/package execution.

The JavaScript query fingerprint is
`c076bf615c407d3cf721a3fd9338a8334e5f3d9af2622e4689b14fb3673c103e` and
`javascript-source-v1` produces extractor fingerprint
`0009595fee74b75e658924893353d56ae10f99f7c9cf6e1afd8b0cfb437aabe2`.
JSX uses the same grammar but query fingerprint
`89fc5f9a8c0ca7bb6471847ad2a5e4bf87cf2c146d4910cdb6366494b7d02fff`
and extractor fingerprint
`1ec8a2320ee127feace3724a36c51486fce2975329ff9e3966937cacfd6dc5ac`.

The TypeScript query fingerprint is
`e0dac24903d2847e85b7ab848d14ef3812384cc13f3b178dbfab539a477e36c0`
and `typescript-source-v1` produces extractor fingerprint
`880ed6928edddfef64955c7160fffeb7f8c6989902d93d690ae59c0a67211d48`.
TSX uses its distinct grammar, query fingerprint
`d8b026a6c1366c2a0f62c5877bd0182e7e73c8ad24ec52ac40a155c2699a61f0`
and extractor fingerprint
`c4440e84b5f84764c8f9dfb42dfc7a0ecb91a1e11ada3aa1aa4d73998b16b3ec`.

Independent expectations assert required declaration roles/signatures, aliases,
module forms, calls, JSX/decorator references, uncertainty labels and forbidden
comment/string names before the exact snapshot comparison. The extractor output is
not used as its own semantic ground truth.

## Commands and evidence

All successful Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME`
and explicit toolchain `PATH` because no system Cargo is on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| `cargo run -p xtask --locked -- verify` (Phase 04 prerequisite, before edits) | 0 | 51 tests, grammar checks, 24 parser fixtures and 8 Rust/Go fixtures clean |
| `cargo check -p ca-languages --all-targets --locked` | 0 | adapter and all language-crate targets compile |
| `cargo run -p xtask --locked -- grammar-check` | 0 | nine providers and 27 total query assets compile; six source extractors advertise readiness |
| `cargo run -p xtask --locked -- js-ts-fixtures` (before golden creation) | 6 | all independent semantic checks passed; missing reviewed golden was reported, then added deliberately |
| `cargo test -p ca-languages --locked` (first run) | 101 | obsolete capture-count expectation exposed by new scope capture; updated to assert the exact bounded scope/declaration split |
| `cargo test -p ca-languages --locked` (final focused run) | 0 | 16/16 language tests pass |
| `cargo run -p xtask --locked -- fixtures` | 0 | 24/24 parser fixtures match the deliberately reviewed golden |
| `cargo run -p xtask --locked -- rust-go-fixtures` | 0 | 8/8 Rust/Go regression cases pass unchanged |
| `cargo run -p xtask --locked -- js-ts-fixtures` | 0 | 11/11 JS/JSX/TS/TSX expectation and exact-hash lines pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | no warnings |
| `cargo test --workspace --locked` | 0 | 54 passed; one storage helper ignored and exercised by its parent test |
| `cargo fmt --all -- --check` | 0 | formatting clean |
| `jq empty config/dependency-lock.json fixtures/expectations.json fixtures/rust-go-expectations.json fixtures/js-ts-expectations.json` | 0 | edited JSON documents valid |
| `cargo run -p xtask --locked -- verify` (final) | 0 | format, clippy, 54-test workspace, 27 query assets, 24 parser cases, 8 Rust/Go cases and 11 JS/TS cases all pass |

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Explicit JS/JSX/TS/TSX routing including module/declaration extensions | passed | routing unit test plus `.cjs`, `.mjs`, `.mts`, `.cts` and `.d.ts` fixture paths |
| TypeScript and TSX use distinct grammars | passed | unequal grammar fingerprints asserted in unit test and grammar-check output |
| Declarations, methods, arrow bindings, scopes, async and generics | passed | independent expectations across core, advanced and TSX cases |
| ESM imports/exports/re-exports and reviewed CommonJS forms | passed | alias, cycle, static/dynamic require/import and export expectations |
| Type/value roles and same-name declarations are not conflated | passed | `Token` interface/value fixture plus ID/role assertions |
| Interfaces, aliases, enums, namespaces, overloads and decorators | passed | TypeScript advanced and `.d.ts` expectations |
| JSX component/intrinsic references are not direct calls | passed | JSX/TSX limitations and unit test prohibit matching JSX call sites |
| Dynamic/computed calls are not overclaimed | passed | all calls unresolved; dynamic module/computed property reason assertions |
| Comments/strings, malformed edits and Unicode ranges | passed | forbidden names, partial parse diagnostics, exact source-byte checks and inherited CRLF/Unicode test |
| Query compilation, bounds and cancellation for affected dialects | passed | grammar-check, bounded capture test and four-dialect cancellation test |
| No Node/npm or JavaScript config execution | passed | Rust-only in-process adapter/harness; no runtime dependency or subprocess path added |
| Rust/Go and shared parser regressions | passed | 8 focused Rust/Go and 24 parser golden entries |
| Linux x64 native extraction | not run | only `aarch64-apple-darwin` is installed locally |
| Windows x64 MSVC native extraction | not run | native runner unavailable locally |

## Known limitations and risks

- This is syntax extraction, not package resolution, type checking, overload
  selection, control-flow analysis, compiler semantics or runtime dispatch.
- ESM re-exports are retained with cycle limitations but not traversed. CommonJS
  observations cover reviewed `require`, `module.exports` and `exports.name` shapes;
  arbitrary metaprogramming remains unresolved.
- `tsconfig`/`jsconfig` paths, package exports, workspace package rules, bundler
  aliases and conditional resolution are not evaluated. No JSON/JSONC config parser
  was needed or added in this phase.
- JSX references cover opening and self-closing tag names without duplicating closing
  tags. Framework transforms and intrinsic/component runtime behavior remain unknown.
- Decorators are syntax references and declaration attributes; behavior is not run
  or inferred. Computed keys and dynamic module/property expressions remain unresolved.
- Structural IDs exclude byte offsets but can change after named-sibling or other
  syntax-tree restructuring, as recorded in ADR-0003.
- The focused corpus establishes deterministic contract coverage, not real-world
  precision/recall. Parser fuzzing and native Linux/Windows execution remain pending.
- Parser work is still synchronous at this crate boundary. The indexing phase must
  place workers behind a bounded CPU queue before protocol use.
- No MCP extraction/search capability is exposed and no observations are persisted yet.

## Blockers and safe next actions

No Phase 05 blocker on the available macOS arm64 host. Do not infer cross-platform
execution, package/path-alias resolution, type checking, JSX runtime calls or dynamic
dispatch from this result. Phase 06 can implement C# and Java under the same syntax-
evidence and explicit-uncertainty boundary.

## Exact next prompt

`prompts/06-csharp-and-java.md`
