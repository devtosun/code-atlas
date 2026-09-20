# Phase 10 report — search, graph queries and bounded context assembly

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git
worktree (`git status --short` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 /
`aarch64-apple-darwin`; Rust 1.98.1
(`48a229ceaefd4985c50990b14116b6d856af0985c`, LLVM 22.1.8)

## Scope implemented

- Added a pure retrieval use-case layer with bounded ranked symbol search,
  declaration lookup, file outlines, references, callers/callees, repository maps,
  reverse impact, authorized source reads and deterministic context assembly.
- Ranked qualified exact, exact, case-folded exact, case-sensitive prefix and folded
  prefix results before FTS5. Identifier tokens split camel/snake/acronym and
  punctuation boundaries. FTS expressions are literal-token only and all SQL values
  remain parameters.
- Added opaque keyset cursors bound to operation, repository, pinned generation and
  normalized query/filters. Malformed, foreign, stale and cross-query cursors are
  distinct failures.
- Preserved graph resolution, candidate count, rule version, reason, limitations and
  coverage. Candidate edges remain opt-in. Traversal is cycle-safe and bounded by
  depth, node, edge and cooperative deadline limits; no-callers results do not claim
  dead code.
- Added current-source excerpts through the existing authorized reader. Indexed and
  live hashes must match; changed, deleted, newly excluded and non-indexed paths are
  explicit outcomes.
- Added deterministic context ranking and overlap deduplication with source hash/range
  provenance, coverage, uncertainty warnings and truncation. Token estimates name the
  `ceil(UTF-8 JSON bytes / 4)` method.
- Counted the exact serialized bytes of structured content plus its JSON text
  fallback. Oversized results drop whole records/lines and reserialize; they never
  slice JSON or UTF-8 bytes.
- Kept schema version 4 and added no third-party dependency. The index configuration
  fingerprint changed to `codeatlas-default-index-config-v2-search-tokens` so derived
  search documents are refreshed.
- Kept the MCP advertisement status-only. Phase 11, not Phase 10, owns rmcp tool
  schemas, error mapping and client integration tests.

## Files changed

- Retrieval policy/domain/tests: `crates/ca-engine/src/{lib,retrieval}.rs`
- Engine dependency declaration/lock metadata: `crates/ca-engine/Cargo.toml`,
  `Cargo.lock`
- Pinned SQLite reads/tests: `crates/ca-storage/src/storage.rs`
- Composition adapter/search derivation: `crates/ca-cli/src/{main,indexer,retrieval}.rs`
- Contracts/state: `README.md`, `README.tr.md`,
  `docs/{ARCHITECTURE,DATA_MODEL,MCP_CONTRACT,PROJECT_STATE,SECURITY_PRIVACY,TEST_MATRIX}.md`
- Static evidence: `config/dependency-lock.json`, `scripts/validate_kit.py`,
  `tests/acceptance-scenarios.json`
- Decision: `docs/adr/0009-generation-bound-retrieval-and-context.md`
- Prior handoff correction: `docs/reports/09-resolution-and-graph.md`

## Decisions and ADRs

ADR-0009 records the generation-bound retrieval port, deterministic ranking and
cursor format, live-source hash verification, context overlap policy and exact
serialized response accounting. SQLite performs bounded generation-scoped reads;
the engine owns application policy; Phase 11 will own only transport adaptation.

No schema migration was needed. Search document derivation changed, so the index
configuration fingerprint changed and prevents an unchanged source file from reusing
an older derived document. `serde`/`serde_json` were already workspace dependencies;
adding them to `ca-engine` introduced no new resolved package.

The referenced filename `09-resolution-and-graph.md` handed off to a nonexistent
`prompts/10-search-and-retrieval.md`. The manifest-authoritative prompt is
`prompts/10-retrieval-and-context.md`; state, validator and the Phase 09 report now
use that name.

## Commands and evidence

All Cargo commands used the pinned temporary `RUSTUP_HOME`/`CARGO_HOME` and explicit
toolchain `PATH` because no system Cargo is on the default PATH.

| Command | Exit code | Result |
|---|---:|---|
| `cargo run -p xtask --locked -- verify` (Phase 09 prerequisite, before edits) | 0 | prior format, lint, workspace and fixture gates clean |
| `git status --short` | 128 | repository metadata unavailable; directory is not a Git worktree |
| `cargo check -p ca-engine --locked` (first attempt) | 101 | lockfile required the newly declared existing workspace deps; refreshed offline |
| `cargo check -p ca-engine --offline` (first compile) | 101 | one missing `Ord` derive on the reference cursor key; corrected |
| `cargo check -p ca-engine --locked` | 0 | retrieval domain/use cases compile |
| `cargo check -p ca-storage --locked` (first compile) | 101 | explicit checked SQLite INTEGER-to-u64 conversion required; corrected |
| `cargo check -p ca-storage --locked` | 0 | generation-scoped retrieval reads compile |
| `cargo check -p ca-cli --locked` (first compile) | 101 | one iterator result wrapper mismatch; corrected |
| `cargo check -p ca-cli --locked` | 0 | storage adapter and search-document derivation compile |
| targeted `pinned_retrieval_search_is_ranked_bounded_and_literal_safe` | 0 | live bundled-SQLite exact rank, filters and punctuation/injection-shaped literals pass |
| targeted `graph_edges_are_generation_scoped_and_candidates_require_opt_in` | 0 | rich references/relations/path impact, candidate evidence and coverage pass |
| `cargo test -p ca-engine retrieval::tests --locked -- --nocapture` | 0 | five adversarial retrieval/context tests pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` (first attempt) | 101 | six style diagnostics in new retrieval code; corrected without changing behavior |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | no warnings |
| `cargo test --workspace --locked` | 0 | 83 tests passed; two child helpers ignored and exercised by parent tests |
| `cargo fmt --all -- --check` | 0 | formatting clean |
| `cargo run -p xtask --locked -- verify` | 0 | workspace and focused fixture gates pass |
| `python3 scripts/validate_kit.py` | 0 | phase/scenario/dependency/report/link state is consistent |
| `jq empty config/dependency-lock.json tests/acceptance-scenarios.json` | 0 | changed JSON artifacts are valid |

## Acceptance criteria

| Criterion | Result | Evidence |
|---|---|---|
| Exact/name/prefix ranks precede FTS | passed | engine ordering and live SQLite tier tests |
| Camel/snake/Unicode/punctuation input is literal-safe | passed | tokenizer unit cases and SQLite punctuation/injection-shaped query cases |
| SQL and traversal stay bounded and parameterized | passed | fixed SQL predicates/parameters, validated limits and candidate caps |
| Symbol, outline, references, caller/callee, map and impact use cases exist | passed | typed `RetrievalService` methods and pinned storage adapter |
| Candidate/uncertainty/coverage evidence is retained | passed | rich graph records, opt-in candidate tests and warnings |
| Cursors bind repository, generation and query | passed | pagination plus cross-query/stale/foreign rejection tests |
| Source reads reject stale/deleted/excluded evidence | passed | current-hash, deletion and hard-exclusion tests |
| Context is deterministic and removes overlapping snippets | passed | ranked duplicate-range fixture |
| Final serialized result obeys byte cap | passed | exact wire projection test with escaping and fallback duplication |
| Linux x64 native retrieval/storage | not run | only `aarch64-apple-darwin` is installed locally |
| Windows x64 MSVC native retrieval/storage | not run | native runner unavailable locally |

Acceptance scenarios P25–P28 are PASS on the available macOS arm64 host.

## Known limitations and risks

- This remains syntax-informed retrieval. Candidate or absent call edges cannot prove
  runtime dispatch, behavior impact, dead code or exhaustive references.
- FTS retrieval uses deterministic lexical tokens only. There are no embeddings,
  vector store, model download or LLM reranker.
- Search database reads have an explicit 10,000-candidate internal cap. Deep result
  sets remain bounded, but a future performance phase may move all tier/cursor logic
  into indexed SQL without changing the application contract.
- Context overlap policy keeps the higher-ranked range rather than merging source
  into a new semantic unit. Freshness failures become warnings for context assembly
  and explicit errors for direct reads.
- The token estimate is a named byte heuristic, not a tokenizer-specific guarantee.
  The byte limit, not that estimate, is authoritative.
- No 100k-declaration p95, throughput or peak-RSS result was measured in this phase.
- Linux and Windows native behavior remains unexecuted on this host. Parser fuzzing,
  watcher behavior and power-loss durability remain later work.
- The public MCP server still exposes only `repository_status`; treating internal
  retrieval APIs as shipped public tools would be incorrect.

## Blockers and safe next actions

No Phase 10 blocker on the available macOS arm64 host. Phase 11 may expose these
use cases through thin official-rmcp handlers, with generated schemas, annotations,
application/protocol error separation, cancellation and both verified client-era
integration tests. It must preserve startup independence and stdout purity.

## Exact next prompt

`prompts/11-mcp-tools.md`
