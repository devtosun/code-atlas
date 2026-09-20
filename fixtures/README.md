# Seed fixtures — parser kernel and source-extraction phases

These original sources provide a small starting corpus for the seven required languages
and JavaScript/TypeScript dialects. `expectations.json` gives hand-authored minimum
declaration expectations and is executed by `cargo run -p xtask --locked -- fixtures`.
The command parses the original bytes, enforces parse/partial expectations, rejects
missing or forbidden names and compares deterministic ranges and fingerprints with
the reviewed `parser-kernel.golden`. This proves the parser kernel, not full language
extraction or semantic support.

Do not run source, package restores, Flutter builds, Java compilers or repository scripts.
The fixtures are parser inputs. Some reference packages are intentionally not installed.
`partial/broken.ts` is intentionally incomplete. `dart/modern.dart` is a reviewed
capability fixture: records, patterns, sealed classes, extension types and switch
expressions parse with the pinned grammar. Non-ASCII Dart identifiers do not; that
precise grammar limitation is covered separately rather than reported as support.

## Fixture review policy
Expected declaration names here are leaf names rather than final symbol IDs. Phase 03
records reviewed declaration kinds, names and exact byte ranges; language-specific
phases must add containers, signatures, scope ownership and count-sensitive cases.
One repeated name may have multiple declarations.
`phantom_call` in comments and strings must not be extracted as a declaration or call.
In shadowing examples, never bind a local parameter/closure call to a same-named imported
or module function. Interface dispatch is a candidate/unresolved relation, not proof.

## Required expansion before release
Each language needs imports/aliases, nested scopes, declarations, real call sites,
negative comments/strings, invalid/incomplete source, Unicode/CRLF and cancellation/reset
cases. Add multi-file ambiguity, overloads, partial declarations, Rust trait dispatch,
Go interfaces, Java virtual calls, C# extension methods, JS re-exports and Dart part files.
Verify package provenance, grammar ABI, query compilation, node shapes and capture limits.

Preserve `unicode/crlf.ts` bytes; it intentionally has CRLF. Other sources are UTF-8 LF.
The harness has no snapshot-update mode: inspect original bytes and query changes, then
edit the golden deliberately. Grammar or query fingerprint changes invalidate the
corresponding lines. Never copy failing output into the golden just to make the gate pass.

## Phase 04 Rust/Go corpus

`rust-go-expectations.json` independently labels declarations, containers, receiver
types, signature fragments, aliases, calls, conditions and negative names across
eight Rust/Go cases. `cargo run -p xtask --locked -- rust-go-fixtures` verifies those
labels and compares exact category counts/hashes with `rust-go.golden`.

The added files cover multiple source files, identical receiver method names,
nested shadowing, generics, traits/interfaces, external imports, closures, Rust
macros/cfg, Go embedded fields/build tags, incomplete edits and Unicode identifiers.
Every extracted call must remain unresolved with no target ID. Macro token trees are
not expanded and cfg/build tags do not assert an active configuration. The standard
`xtask verify` command runs the original parser golden and every implemented focused
language gate.

## Phase 05 JavaScript/TypeScript corpus

`js-ts-expectations.json` independently labels declarations, type/value roles,
signatures, ESM/CommonJS imports and exports, aliases, unresolved calls, JSX
references, decorators, uncertainty limitations and negative names. `cargo run -p
xtask --locked -- js-ts-fixtures` checks eleven JavaScript, JSX, TypeScript and TSX
cases and compares exact category counts/hashes with `js-ts.golden`.

The focused files cover alias imports, cyclic re-exports, recognized CommonJS,
static and dynamic `import`/`require`, shadowing, optional/computed calls, anonymous
callbacks, JSX intrinsic/component tags, same-name type/value declarations,
interfaces/type aliases/enums/namespaces, overloads, decorators, TSX generics,
`.d.ts`/`.mts`/`.cts` routing, malformed edits and Unicode. JSX tags carry an
explicit non-call limitation; every call target remains unresolved. The harness
never invokes Node, npm, JavaScript configuration, scripts or a module loader.

## Phase 06 C# and Java corpus

`csharp-java-expectations.json` independently labels declarations, containers,
signatures, partial-group evidence, regular/static imports, unresolved receiver
calls, attributes/annotations, Java method references, C# preprocessor conditions
and negative names. `cargo run -p xtask --locked -- csharp-java-fixtures` checks
eleven C#/Java cases and compares exact category counts/hashes with
`csharp-java.golden`.

The focused files cover block and file-scoped namespaces, two separate C# partial
declarations, records/enums/interfaces, properties/accessors, fields, constructors,
generic overloads, async/local/extension methods, top-level code, preprocessor
branches, Java nested types, lambdas and method references. Interface/member calls
retain receiver expression source but remain unresolved. Attribute/annotation
arguments and DI-like strings are never executed or promoted to call edges. The
corpus also includes malformed recovery plus Unicode CRLF files. The harness never
invokes .NET, a JVM, compilers, MSBuild, Gradle/Maven, dependency restore or project
evaluation.

## Phase 07 Dart corpus

`dart-expectations.json` independently labels declarations, containers,
signatures, library/import/export/part relationships, aliases, unresolved calls,
constructor-like evidence, references, diagnostics and negative names. `cargo run
-p xtask --locked -- dart-fixtures` checks eight Dart cases and compares exact
category counts/hashes with `dart.golden`.

The focused files cover libraries and parts, package aliases, ordinary/named/
const/factory/redirecting constructors, getters/setters, generic and async
functions, lexical shadowing, records, patterns, sealed classes, extension types,
switch expressions and nested Flutter-style calls with named arguments and
callbacks. Every call remains unresolved. Capitalized calls are only
`constructor_like`; import prefixes, part links and Flutter widget nesting are
syntax evidence rather than semantic bindings or a widget tree.

Malformed recovery, multibyte UTF-8 in comments/strings, CRLF byte positions and
cooperative cancellation/reset are covered. The pinned grammar reports `ERROR`
for non-ASCII Dart identifiers, so the suite asserts the diagnostic and limitation
instead of claiming support. The harness never invokes a Dart/Flutter SDK,
analyzer, pub, build_runner, package configuration or generated code.
