# Phase 09 report — scope/import resolution and evidence-backed graph

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git
worktree (`git status --short` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 /
`aarch64-apple-darwin`; Rust 1.98.1
(`48a229ceaefd4985c50990b14116b6d856af0985c`, LLVM 22.1.8)

## Scope implemented

- Added a pure engine resolver with deterministic lexical scope chains, shadowing,
  aliases, narrow same-file/module/import rules and bounded re-export traversal.
  Every output retains source observation/range, rule and resolver versions,
  evidence, candidate count and an honest resolution label.
- Preserved overload sets and receiver uncertainty as candidates. Dynamic/computed
  targets and unsupported mappings remain unresolved. Static external imports become
  external observations rather than fabricated local declarations.
- Added inert, authorized reads for root `go.mod` module and `pubspec.yaml` package
  declarations. No compiler, SDK, package manager, build script, module loader or
  repository instruction is executed.
- Wired resolution into every candidate generation after parsing and before commit.
  Incremental jobs intentionally re-resolve the entire candidate graph and share the
  indexing cancellation token.
- Migrated SQLite to schema version 4 with resolver metadata/counts,
  `external_nodes`, `resolved_edges`, `resolution_diagnostics` and composite
  generation-membership foreign keys. Writer validation rejects stale or foreign
  local endpoints, and activation rejects a generation without resolver metadata.
- Added bounded incoming/outgoing neighbor and call-site read ports. Candidate edges
  are excluded by default, explicit opt-in includes them, and traversal enforces
  cycle-safe depth/node/edge limits.
- Added 35 independent expected resolution labels, five for each required language,
  plus a JavaScript recursive re-export cycle and a separate rename/delete/export
  mutation corpus. Incremental final graph facts equal a clean full final index.
- Kept the public MCP surface status-only. Phase 09 implements internal engine and
  storage graph boundaries, not placeholder public search or graph tools.

## Files changed

- Engine: `crates/ca-engine/src/{lib,indexing,repository,resolution}.rs`
- Persistence: `crates/ca-storage/src/{schema,storage}.rs`
- CLI mapping/evaluation: `crates/ca-cli/src/indexer.rs`,
  `crates/ca-cli/tests/cli.rs`
- Labelled corpora: `fixtures/resolution-expectations.json`,
  `fixtures/javascript/cycle-consumer.js`, `fixtures/resolution-mutations/**`
- Evaluation artifact: `docs/reports/artifacts/09-resolution-evaluation.json`
- Contracts/state: `README.md`, `README.tr.md`, `config/dependency-lock.json`,
  `docs/{ARCHITECTURE,DATA_MODEL,DEPENDENCY_POLICY,LANGUAGE_SUPPORT,MCP_CONTRACT,PROJECT_STATE,TEST_MATRIX}.md`
- Static evidence: `scripts/validate_kit.py`, `tests/acceptance-scenarios.json`
- Decision: `docs/adr/0008-evidence-backed-resolution-graph.md`

## Decisions and ADRs

ADR-0008 records the generation-scoped binding model, provenance requirements,
candidate-by-default uncertainty boundary, inert manifest policy, full-generation
re-resolution and bounded graph ports. Engine domain types depend on neither SQLite
nor Tree-sitter; the CLI composition root maps immutable stored observations into
resolver input and maps the output into one writer-owned transaction.

Only implemented source-level evidence can produce `lexically_resolved`. Phase 09
does not emit `semantically_resolved`. Method-name matching cannot prove receiver,
interface, trait, virtual or extension dispatch. Package export maps, tsconfig-style
aliases, active build conditions and compiler type inference remain explicit gaps.

The storage observation spelling cap was raised from 512 to 4096 bytes after a real
index of this repository found a legitimate 532-byte Rust call expression. The cap
remains bounded, the rejection reports exact lengths/path, and no assertion was
weakened to accept unbounded input.

## Commands and evidence

All Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME` and explicit
toolchain `PATH` because no system Cargo is on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| `cargo run -p xtask --locked -- verify` (Phase 08 prerequisite, before edits) | 0 | prior format, lint, workspace and fixture gates clean |
| `cargo test -p ca-engine --locked` | 0 | pure resolver lexical/import/candidate/cycle/graph-port tests pass |
| `cargo test -p ca-storage --locked` | 0 | 13 passed; two child helpers ignored and exercised by parent tests |
| targeted `hand_labelled_resolution_corpus_matches_per_language_metrics` | 0 | all 35 expected sites and checked-in per-language metrics match |
| targeted `changed_file_and_clean_full_index_converge_after_rename_delete_and_export_change` | 0 | incremental and clean full final graph facts are identical |
| manual `codeatlas index` over `fixtures/` in isolated application data | 0 | schema v4 generation resolves and activates |
| manual `codeatlas index` over this repository in isolated application data | 0 | 89 files parse/persist/resolve; unsupported/cycle cases remain explicit warnings |
| `cargo fmt --all -- --check` (first final attempt) | 1 | formatting-only differences; applied `cargo fmt --all` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` (first final attempt) | 101 | one collapsible manifest-parser branch; corrected and formatted |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` (final) | 0 | no warnings |
| `cargo test --workspace --locked` (first final attempt) | 101 | one interrupted-writer recovery assertion observed `building`; the exact test and complete storage crate immediately passed in isolation |
| `cargo test -p ca-storage --locked storage::tests::reopen_marks_interrupted_staging_without_damaging_active_data -- --exact --nocapture` | 0 | recovery test passes in isolation |
| `cargo test -p ca-storage --locked` | 0 | all 13 ordinary storage tests pass; two helpers ignored |
| `cargo fmt --all -- --check` (final) | 0 | formatting clean |
| `cargo test --workspace --locked` (final) | 0 | 77 passed; two child helpers ignored and exercised by parent tests |
| `cargo run -p xtask --locked -- verify` (final) | 0 | format, clippy, all workspace tests, 36 query assets and every focused fixture suite pass |
| `python3 scripts/validate_kit.py` (final) | 0 | phase/fixture/scenario manifests, TOML examples, 67 Markdown files/links and Phase 09 dependency evidence are consistent |
| `jq empty fixtures/resolution-expectations.json docs/reports/artifacts/09-resolution-evaluation.json config/dependency-lock.json tests/acceptance-scenarios.json` | 0 | all Phase 09 JSON artifacts are valid |

## Evaluation artifact

The independently authored corpus contains five labelled sites per required language
(35 total). The test compares exact site identity, resolution label, target multiset
and candidate count; the artifact is not generated from resolver output during the
test.

| Language | Syntax coverage | Binding precision | Binding recall | Unresolved rate | Candidate sites | External observations |
|---|---:|---:|---:|---:|---:|---:|
| C# | 5/5 | 6/6 | 6/6 | 0/5 | 3 | 1 |
| Dart | 5/5 | 3/3 | 3/3 | 1/5 | 1 | 1 |
| Go | 5/5 | 4/4 | 4/4 | 0/5 | 2 | 1 |
| Java | 5/5 | 5/5 | 5/5 | 0/5 | 3 | 1 |
| JavaScript | 5/5 | 3/3 | 3/3 | 2/5 | 0 | 0 |
| Rust | 5/5 | 4/4 | 4/4 | 0/5 | 1 | 1 |
| TypeScript | 5/5 | 4/4 | 4/4 | 1/5 | 0 | 0 |

These exact small-corpus results prove conformance to the implemented rule set. They
are not production-repository, ecosystem-wide or compiler-level accuracy scores.
JSX and TSX retain parser regression coverage but are aggregated under their language
families rather than assigned unsupported independent accuracy claims.

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Deterministic lexical/import cases resolve correctly | passed | exact 35-site corpus plus unit tests for shadowing, aliases and re-exports |
| Ambiguous/dynamic/interface-style cases preserve uncertainty | passed | overload/receiver candidate sets and computed/dynamic unresolved labels match independent expectations |
| Edges belong only to the selected generation | passed | composite membership FKs, writer validation and stale-target storage test |
| Deletes leave no stale targets | passed | deletion activation plus rename/delete/export mutation equivalence test |
| Candidate opt-in is enforced | passed | graph/storage tests exclude by default and include only with explicit flag |
| Graph cycles and budgets are bounded | passed | re-export cycle and BFS depth/node/edge truncation tests |
| Full and changed-file graphs are equivalent | passed | canonical stored graph facts match after the mutation corpus |
| Per-language syntax/binding/unresolved metrics are separate | passed | checked-in JSON artifact with numerator/denominator sample sizes |
| Linux x64 native resolution/storage | not run | only `aarch64-apple-darwin` is installed locally |
| Windows x64 MSVC native resolution/storage | not run | native runner unavailable locally |

## Known limitations and risks

- This is syntax-informed lexical/import resolution, not compiler or runtime
  semantics. No type inference, overload selection, macro expansion, virtual/trait/
  interface dispatch, dependency injection or cross-language call inference exists.
- Module support is intentionally narrow. Node/package export maps, tsconfig paths,
  Dart package configuration and parts, Rust cargo/crate configuration, Go workspace
  rules, Java classpaths and C# project/assembly references remain unsupported.
- Full-generation re-resolution favors consistency over large-repository latency.
  Phase 09 has deterministic bounds but no 10k/100k timing or peak-RSS claim.
- Resolver cancellation is cooperative. Native Tree-sitter memory-safety isolation
  and parser fuzzing remain separate hardening work.
- The first workspace-wide test run saw one non-resolution interrupted-writer
  recovery timing failure; immediate exact and full storage reruns passed. If it
  recurs, the recovery/lock test synchronization should be hardened rather than
  dismissed as semantic evidence.
- Public search, pagination, result byte budgets and MCP graph tools remain Phase 10
  and later work. Internal read ports do not make those user-facing capabilities.
- Linux and Windows native behavior remains unexecuted on this host.

## Blockers and safe next actions

No Phase 09 blocker on the available macOS arm64 host. Do not infer cross-platform
execution or compiler-level semantics from the
fixture evaluation. Phase 10 may build ranked retrieval, freshness checks, stable
pagination and bounded public graph use cases over the selected generation.

## Exact next prompt

`prompts/10-retrieval-and-context.md`
