# Phase 06 — C# and Java extraction

$ca-phase-driver $ca-treesitter-language $ca-graph-resolution

## Task and scope
Complete C# and Java syntax extraction, prioritizing enterprise code constructs and correct declaration identities.

## Prerequisites
Phase 05 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/DATA_MODEL.md`
- `fixtures/README.md`

## Implementation steps
1. Inspect grammar nodes and create compiled per-language capture queries; maintain
   normalized scopes/declarations while preserving language-specific attributes.
2. C#: namespace/file-scoped namespace, class/interface/record/enum, partial types,
   fields/properties/accessors, constructors, extension methods, generic/overloaded
   methods, async, attributes, local functions, top-level code and preprocessor
   observations. Store each partial declaration separately with evidence for grouping.
3. Java: package/import/static import, class/interface/enum/record, nested types,
   constructors, generic and overloaded methods, fields, annotations, lambdas and
   method references. Keep declaration identity separate from simple method name.
4. Record call/constructor/reference sites, containing scope and receiver expression
   source. An interface field's method call is not a proof of an implementation
   target; neither is a dependency-injection registration inferred from a string.
5. Add multi-file fixtures for partial declarations, overload candidates, nested
   classes, extension calls, annotation/attribute arguments, preprocessor branches,
   same-name methods and comments/strings. Preserve Unicode/CRLF source ranges.
6. Surface grammar limitations on newer syntax; do not pretend an ERROR subtree is
   fully understood. Record capabilities separately for parse and extraction.

## Acceptance gates
- Required C#/Java construct fixtures pass with distinct IDs and accurate ranges.
- Partial/overloaded/nested declarations are not collapsed by name-only keys.
- Interface/virtual/DI calls retain uncertainty; no invented implementation edges.
- No .NET/JVM, restore, MSBuild or Gradle execution is required for indexing.
- Syntax-error coverage and negative-reference fixtures pass.
- Full previously supported language suite remains green.

## Out of scope
No VB.NET, Roslyn/JDT semantic processing, .csproj/Gradle evaluation or framework route inference.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/06-csharp-and-java.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
