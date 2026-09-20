# Phase 00 — Feasibility, versions and compatibility

$ca-phase-driver $ca-treesitter-language $ca-mcp-contract

## Task and scope
Build a disposable but runnable compatibility spike before designing around
unverified dependency combinations. Establish one compatible Tree-sitter runtime
for all seven languages, plus JSX/TSX, official rmcp interop and patched SQLite/FTS5.

## Prerequisites
No implementation prerequisites; inspect the kit and host first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/DEPENDENCY_POLICY.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/GRAMMAR_MATRIX.md`
- `docs/SOURCES.md`

## Implementation steps
1. Inspect rustc/cargo/host target and native compiler availability. Do not install
   software, elevate privileges or change system execution policy silently. Record
   missing prerequisites with platform-specific remediation.
2. Create `spikes/compatibility` as an isolated Cargo project/workspace. Select a
   stable toolchain and published dependencies from official sources. Inspect each
   grammar's actual metadata, source, license, bindings and ABI. Resolve Dart package
   provenance explicitly; do not assume the similarly named repo is the crate source.
3. Compile ALL required grammar bindings in the same executable. Test set_language,
   real nonempty fixtures and at least one compiled query per dialect. Print/record
   ABI/runtime intervals in the spike only, never contaminate an MCP server's stdout.
   Test modern Dart syntax separately from basic Dart; record unsupported constructs.
4. If an upstream binding is incompatible, evaluate a maintained compatible release
   or a reviewed vendored grammar snapshot with pinned generator/build inputs. No
   unsafe Language cast, regex fallback, floating Git main or runtime download.
5. Build a tiny rmcp status server and subprocess client. Test modern discovery and
   per-request metadata for 2026-07-28 and legacy initialize for 2025-11-25 using the
   pinned SDK. Record exact release/features and installed Codex version if available.
6. Open bundled SQLite, query its version/compile options, enforce the known-fix
   policy, create an FTS5 table and execute a real insert/search/delete transaction.
7. Populate GRAMMAR_MATRIX.md, generate `config/dependency-lock.json`, commit-worthy
   Cargo.lock and a compatibility report with source links, license and execution
   evidence. Native platforms not available locally remain explicitly untested.

## Acceptance gates
- One build contains Dart, C#, Rust, Go, Java, JavaScript/JSX, TypeScript and TSX.
- All basic fixtures parse without unexpected ERROR/MISSING nodes and queries run.
- Every grammar has verified provenance, ABI and pinned version/commit.
- Both MCP lifecycle tests pass; their raw frames are version-correct.
- Actual bundled SQLite passes version/FTS5 checks, not merely crate metadata.
- A reproducible command reruns the spike with --locked. No guessed compatibility.

## Out of scope
No full indexer, search engine, user config writes, mandatory daemon or optional LSP.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/00-compatibility-spike.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
