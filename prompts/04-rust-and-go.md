# Phase 04 — Rust and Go language extraction

$ca-phase-driver $ca-treesitter-language $ca-graph-resolution

## Task and scope
Complete source-level Rust and Go declaration/scope/import/reference/call-site extraction with negative cases and honest limits.

## Prerequisites
Phase 03 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/DATA_MODEL.md`
- `fixtures/README.md`

## Implementation steps
1. Inspect pinned grammar node types and add separate symbols/imports/references/
   calls queries for Rust and Go. Compile queries in the fixture harness.
2. Rust: cover modules, use aliases, structs/enums, traits, impl/trait impl, associated
   methods, generics, async, closures, macros and cfg observations. Do not expand
   macros, execute build scripts or infer active cfg without configuration evidence.
3. Go: cover package/import aliases, types, structs/interfaces, receiver methods,
   functions, generics, embedded fields, closures, constructor-like function calls
   and build-tag observations. Record receiver type source and function signature.
4. Generate declaration IDs independent of line insertion where possible; distinguish
   same-name receiver methods and nested/shadowed bindings. Bind containing scopes,
   not arbitrary project-wide name matches. Keep call targets unresolved at this layer.
5. Add multi-file positive fixtures and negative comments/strings/identical method
   names, generics/traits/interface ambiguity, external imports, invalid edits and
   Unicode positions. Seed expectations must be independently reviewed.
6. Update language capabilities and extraction fingerprints. Run every previously
   passing dialect/parser regression as well as focused Rust/Go fixtures.

## Acceptance gates
- Expected declarations, containers, aliases and call sites are extracted exactly.
- Comments/string literals are not references; shadowed bindings stay distinct.
- Trait/interface/method-name matches are not claimed as proven runtime calls.
- Macro/build-configuration limitations are surfaced in output/documentation.
- Rust/Go fixture, determinism, budget and cancellation gates pass.
- Earlier parser compatibility tests remain green.

## Out of scope
No rust-analyzer/gopls requirement, cargo/go build, macro expansion or project-wide resolver implementation.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/04-rust-and-go.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
