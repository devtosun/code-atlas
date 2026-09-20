# Phase 07 — Dart and Flutter-style source extraction

$ca-phase-driver $ca-treesitter-language $ca-graph-resolution

## Task and scope
Complete the required Dart adapter rather than leaving Dart as a parser-only placeholder. Flutter syntax is covered as Dart, not as a running Flutter application.

## Prerequisites
Phase 06 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/GRAMMAR_MATRIX.md`
- `docs/DEPENDENCY_POLICY.md`
- `fixtures/README.md`

## Implementation steps
1. Recheck the chosen Dart grammar's package provenance, pinned ABI and syntax
   coverage from phase 00. Inspect current node-types and real S-expressions.
   Do not copy capture names from a different Dart fork or an old online snippet.
2. Extract libraries/imports/exports, aliases, part/part of, classes/enums/mixins,
   extensions, functions/methods, named/redirecting/factory constructors, getters/
   setters, async/await, generic signatures and source scopes.
3. Add fixtures for records, patterns, sealed classes and extension types as required
   coverage targets. Where the pinned grammar cannot support a construct, attempt
   a reviewed compatible grammar fix/version within scope; otherwise mark the
   precise limitation/blocker. Never silently switch to regex or claim full Dart 3.
4. Cover nested Flutter-style widget constructors, build methods, named arguments,
   callbacks and import prefixes without requiring the Flutter SDK or executing code.
   Widget nesting is syntax evidence, not a proven navigation route/call graph.
5. Handle declaration identifiers vs member/constructor references and lexical
   shadowing carefully. Preserve ambiguous method targets and external package URIs.
6. Add complete positive/negative/partial files, Unicode/CRLF and timeout/reset
   cases. Update the seven-language release matrix and fingerprint invalidation tests.

## Acceptance gates
- Dart has real declarations, scopes, imports and call-site extraction tests.
- Basic and supported modern Dart fixtures parse and extract expected facts.
- Unsupported modern forms are named explicitly and never counted as supported.
- Flutter-style fixtures require no Dart/Flutter runtime at indexing time.
- Prefix aliases, getters/setters and constructor identities are not conflated.
- All seven languages plus JSX/TSX pass their applicable extraction gates.

## Out of scope
No Dart analyzer sidecar, pub get/build_runner, semantic Flutter tree or automatic grammar download.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/07-dart.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
