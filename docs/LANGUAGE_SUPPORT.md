# Language support contract

Seven required languages, with JSX and TSX treated as dialect test targets.
“Supported” must separately describe parse coverage, declaration extraction,
reference/call-site observation and name-resolution coverage. Grammar loading alone
is not product support. `docs/GRAMMAR_MATRIX.md` records actual tested versions.

## Providers selected in phase 00
| Language | Verified upstream/package |
|---|---|
| Dart | `nielsenko/tree-sitter-dart` / published `tree-sitter-dart` 0.2.0; UserNobody14 is a distinct alternative |
| C# | tree-sitter/tree-sitter-c-sharp / tree-sitter-c-sharp |
| Rust | tree-sitter/tree-sitter-rust / tree-sitter-rust |
| Go | tree-sitter/tree-sitter-go / tree-sitter-go |
| Java | tree-sitter/tree-sitter-java / tree-sitter-java |
| JavaScript/JSX | tree-sitter/tree-sitter-javascript / tree-sitter-javascript |
| TypeScript/TSX | tree-sitter/tree-sitter-typescript / tree-sitter-typescript; separate TS and TSX grammar values |

Sources: S07–S15 in `docs/SOURCES.md`. Upstream main branches are inspection
sources, never unpinned production dependencies. Exact published versions, crate
checksums, source commits and ABIs are in `docs/GRAMMAR_MATRIX.md` and
`config/dependency-lock.json`.

## Production readiness through Phase 13

| Provider | Parser | Query/extractor status | Bounded resolver status |
|---|---|---|---|
| Dart | ready | declarations, scopes, library/import/export/part relationships, references and calls ready | lexical names, relative imports and `pubspec.yaml` package roots; package configuration, parts and dispatch remain limited |
| C# | ready | declarations, scopes, usings, references, calls and preprocessor observations ready | lexical names and namespace/import candidates; partial groups, overload selection and dispatch stay candidates |
| Rust | ready | declarations, scopes, imports, references, calls and cfg conditions ready | lexical names, source modules and aliases; cfg selection, traits and dispatch remain limited |
| Go | ready | declarations, scopes, imports, references, calls and build tags ready | lexical names plus same-module imports from root `go.mod`; build selection and receiver dispatch remain candidates |
| Java | ready | declarations, scopes, package/imports, references, calls and method references ready | lexical names plus package/import candidates; overload and virtual/interface dispatch stay candidates |
| JavaScript | ready | declarations, scopes, ESM/CommonJS observations, references and calls ready | lexical names, relative modules, aliases and bounded re-exports; package exports/path aliases/dynamic targets unsupported |
| JSX | ready, explicit dialect | JavaScript extraction plus JSX tag references ready | JavaScript rules apply; JSX tags remain references and are never promoted to calls |
| TypeScript | ready | JS forms plus type/value roles, TS declarations, overloads and decorators ready | ECMAScript rules with role-aware candidates; no compiler type checking or tsconfig paths |
| TSX | ready, dedicated grammar | TypeScript extraction plus JSX references and TSX generics ready | TypeScript rules apply; JSX tags remain references and are never promoted to calls |

Every provider advertises `parser_ready=true`, `extractor_ready=true` and readiness
for declarations/scopes/imports/references/calls. Those provider flags describe the
syntax adapter. Phase 09 adds a separate `codeatlas-resolver-v1` pass; it does not
turn a Tree-sitter provider into a compiler semantic service or claim complete
whole-ecosystem resolution.

Phase 09 persists resolver output for every activated candidate generation and
validates the seven required languages with five independently labelled sites each.
The corpus also exercises the JSX/TSX-backed ECMAScript adapters in the ordinary
parser regression suite, but the accuracy artifact aggregates them under JavaScript
and TypeScript rather than inventing separate five-site dialect scores.

Phase 10 does not change grammar, extraction or resolver readiness. Retrieval keeps
language, parse status, coverage, resolution, candidates and limitations visible;
ranked search or an absent call edge does not upgrade syntax evidence into compiler
semantics.

Dart coverage includes libraries, imports/exports and aliases, part/part-of syntax,
classes/enums/mixins/extensions/extension types, functions/methods, named/factory/
redirecting constructors, getters/setters, generics, async/await, records, patterns,
sealed classes and switch expressions. Capitalized calls are constructor-like syntax,
not proven constructor or Flutter widget targets. Nested Flutter-style calls, named
arguments and callbacks are retained without constructing a semantic widget tree.
No Dart/Flutter SDK, analyzer, pub, build_runner, package configuration or generated
code is loaded or executed. The pinned grammar reports ERROR nodes for non-ASCII
Dart identifiers; this is an explicit unsupported form, while multibyte UTF-8 in
comments/strings and CRLF coordinates remain supported.

Rust coverage includes modules, grouped/aliased use declarations, structs, enums,
traits, inherent and trait impls, associated methods, generics, async functions,
closures, macro declarations/invocations and cfg/cfg_attr observations. Go coverage
includes packages, import aliases, generic types, structs/interfaces, embedded
fields, receiver methods, functions, closures, constructor-like call syntax and
build tags. Receiver type source and function signatures are retained. Neither
adapter expands macros, runs build tooling, loads packages, infers an active build
configuration or proves dynamic dispatch.

JavaScript/JSX coverage includes declarations, classes/methods, named and arrow-
function bindings, parameters/scopes, ESM imports/exports/re-exports, recognized
`require`/`module.exports` forms, optional calls, constructors and async syntax.
TypeScript/TSX adds interfaces, type aliases, enums, namespaces, overload signatures,
generics, type-only imports and decorators. Explicit `role:type`, `role:value` and
`role:namespace` attributes keep same-name type/value declarations distinct. JSX
opening/self-closing tags are reference observations with a `jsx_tag_not_a_call_edge`
limitation. Computed property calls and dynamic module specifiers remain unresolved.
No adapter executes JavaScript, config, package scripts or module loaders. Phase 09
resolves bounded relative paths and re-exports only; path aliases and package export
maps remain explicit diagnostics.

C# coverage includes block and file-scoped namespaces, classes/interfaces/enums/
records, distinct partial declarations with unconfirmed grouping hints, fields,
properties/accessors, constructors, overloads, generics, async/local/extension
methods, attributes, top-level code and preprocessor conditions. Java coverage
includes packages, regular/static imports, classes/interfaces/enums/records and
nested types, fields, constructors, generic/overloaded methods, annotations,
lambdas and method references. Receiver expression source is retained for member
calls, but it is not an inferred type. Interface, virtual and extension dispatch,
overload selection, DI strings, compiler identities and active preprocessor branches
remain unresolved. No .NET/JVM/compiler/build/restore/project tooling is invoked.

## Required syntax fixtures
| Language | Positive and negative coverage |
|---|---|
| Dart | class, enum, mixin, extension/extension type if supported, named/redirecting constructors, factory, async, getters/setters, records, patterns, imports with aliases, part/part of, Flutter-style nested widget constructors; unsupported newer syntax explicitly flagged |
| C# | namespace/file-scoped namespace, partial class, record, interface, properties/accessors, constructors, extension methods, generics, overloaded methods, async, attributes, local functions, top-level statements, preprocessor branches |
| Rust | modules, structs/enums, traits, impl/trait impl, associated methods, generic functions, use aliases, macros, closures, async, cfg; no implicit macro expansion |
| Go | package, import aliases, type/struct/interface, receiver methods, functions, generics, closures, embedded types, build tags; no false resolution from just matching method names |
| Java | package/import/static import, class/interface/enum/record, nested classes, constructors, overloads, annotations, generic methods, lambdas/method references; inheritance does not identify dynamic dispatch |
| JavaScript | declarations, classes, arrow/function-valued bindings, methods, ESM import/export/re-export, CommonJS recognized forms, optional chaining, async, JSX; computed properties/dynamic require remain uncertain |
| TypeScript | JS forms + interfaces, type aliases, enums, namespaces, overloads, generics, decorators as supported, type-only imports, re-exports, TSX components; ambiguous alias/path rules are not guessed |

Extensions: `.dart`, `.cs`, `.rs`, `.go`, `.java`, `.js`, `.jsx`, `.mjs`, `.cjs`,
`.ts`, `.tsx`, `.mts`, `.cts`. `.d.ts` is TypeScript declaration input. `.csx` is
optional, explicitly separate. Vue/Svelte single-file components, VB.NET, Kotlin,
HTML, SQL, build output and notebook injections are outside this release.

## Capture contract
Every provider embeds reviewed `symbols.scm`, `imports.scm`, `references.scm` and
`calls.scm` assets inspected against its pinned node-types and fixture parse shapes.
Query capture names use
`<category>.<syntax-kind>.<role>` where category is declaration, scope, import,
reference, call or condition and role is `name` or `node`. Unknown contracts are
rejected with language/query context. Empty/guessed query modules are not support.

A normalized extraction result owns: language/dialect; file fingerprint; declarations;
lexical scopes; imports/exports; reference occurrences; call/constructor sites;
diagnostics and capability/coverage notes. Each occurrence includes the exact
source range and source spelling. Preserve both syntactic roles and actual symbol
kinds; do not turn every identifier into a reference.

The owned result includes structural observation ID, exact name range, containing
syntax range, scope ID, optional container/signature/receiver/alias/target,
resolution category, attributes and limitations. All nine provider targets populate
the supported collections. Returned ranges are zero-based, half-open UTF-8
bytes over original input; human lines are one-based and byte columns zero-based.
UTF-8 BOM and CRLF bytes are not stripped or normalized. Invalid UTF-8 is rejected.

Implemented Phase 09 rules for binding:
- Resolve lexical scopes before searching file/module/project scopes.
- Keep names in comments/strings out of symbol reference counts.
- Store unresolved external imports and dynamic calls as observations.
- Imports/re-exports need cycle guards and bounded candidate sets.
- Receiver method names alone do not identify a declaration in another type.
- Trait/interface/virtual/DI dispatch cannot be proven by matching names.
- Preserve overload candidates unless evidence uniquely identifies one.
- Flags like cfg, build tags and preprocessor conditions are recorded; inactive
  code is not called active without a known configuration.
- Cross-language string names/routes do not prove a call edge.

Supported root manifests are deliberately narrow: the resolver reads only root
`go.mod` module declarations and root `pubspec.yaml` package names as inert UTF-8
data. It does not run Go, Dart, Node, Java, .NET, Rust tooling or parse executable
configuration. Relative path and module candidates are extension-aware and bounded;
missing/unsupported mappings remain visible diagnostics. Full graph re-resolution
currently runs after every changed-file generation so incremental and clean indexing
converge without a partially invalidated dependency graph.

## Honest resolution labels
`syntax_observation`: a declaration/reference/call exists at this range.
`lexically_resolved`: bounded scope/import rules identified a static declaration.
`candidate`: one or more heuristic targets; never a confirmed call edge by default.
`semantically_resolved`: reserved for a verified optional semantic provider that
identifies a static symbol in a matching file/version/configuration; Phase 09 does
not emit this label. It would still not guarantee runtime execution.
`unresolved`: no supported binding rule; retain the original site and reason.

Use reason, resolver version, candidate_count and evidence rather than an invented
probability. `trace_calls` excludes candidate edges unless explicitly requested.

## Language DoD
Every required adapter passes grammar load + full parse + query compile + positive
symbol/import/call-site extraction + negative references + incomplete source +
Unicode/CRLF positions + cancellation/budget + deterministic IDs tests. Modern
constructs that a grammar cannot parse are visible limitations and BLOCK a promise
of that construct, not a reason to silently remove the language.
