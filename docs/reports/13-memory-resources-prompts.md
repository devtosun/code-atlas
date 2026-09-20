# Phase 13 report — explicit project memory, resources and prompt templates

State: completed
Date: 2026-09-20
Git revision and local diff: unavailable; the supplied directory is not a Git
worktree (`git status --short` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 / aarch64-apple-darwin /
rustc 1.98.1 (48a229cea), LLVM 22.1.8

## Scope implemented

- Migrated SQLite to schema v5. Memories now store decision/convention/pitfall/task
  kind, text, revision, author, origin, scope and timestamps independently of index
  generations. Existing version-one notes migrate with explicit legacy defaults.
- Added bounded transactional upsert/delete with exact optimistic revisions, 16 KiB
  text, 10,000-record, 32-evidence/8 KiB aggregate evidence and metadata/search
  limits. FTS remains trigger-maintained and response reduction removes whole
  memory records.
- Evidence stores exact repository-relative path/content hash and optional immutable
  symbol observation ID. Writes validate the active generation; reads report
  verified, unverified or stale. No same-name rebinding occurs after a rename.
- Added read-only `search_memories`; trusted `serve --memory-write` exposes and also
  backend-authorizes `upsert_memory` and destructive `forget_memory`. The default
  surface has 14 tools; the opted-in surface implements all 16.
- Added a `ca-engine::memory` use-case boundary with bounded validation and a narrow
  store port; the CLI adapter maps it to SQLite while storage repeats critical
  validation as defense in depth.
- Added `codeatlas://repo/status`, `codeatlas://repo/map`, symbol and memory resource
  forms through rmcp. Strict parsing rejects unrestricted schemes, encoding,
  separators and traversal; resources delegate to existing backend use cases.
- Added `explain_symbol`, `plan_change` and `investigate_failure` static MCP prompt
  templates. They quote arguments, request evidence/uncertainty, label retrieved
  text as untrusted and defer approval decisions to the client. No LLM runs inside
  the server and no client UI exposure is assumed.
- Added modern and legacy real-stdio coverage for discovery, all opted-in tools,
  resources/templates/read, prompts/list/get, policy-disabled writes, revision and
  evidence errors, oversized text, stale filtering, reindex/restart retention,
  exact deletion and synthetic prompt-injection content.

## Files changed

- `crates/ca-engine/src/lib.rs`, `crates/ca-engine/src/memory.rs`
- `crates/ca-storage/src/schema.rs`, `crates/ca-storage/src/storage.rs`
- `crates/ca-mcp/src/lib.rs`
- `crates/ca-cli/src/main.rs`, `crates/ca-cli/src/mcp_backend.rs`, `crates/ca-cli/src/memory.rs`
- `crates/ca-cli/tests/cli.rs`, `crates/ca-cli/tests/lifecycle.rs`
- `docs/adr/0012-explicit-memory-resources-and-prompts.md`
- `docs/ARCHITECTURE.md`, `docs/DATA_MODEL.md`, `docs/DEPENDENCY_POLICY.md`
- `docs/MCP_CONTRACT.md`, `docs/SECURITY_PRIVACY.md`, `docs/PROJECT_STATE.md`
- `docs/TEST_MATRIX.md`, `docs/GRAMMAR_MATRIX.md`, `docs/LANGUAGE_SUPPORT.md`
- `README.md`, `README.tr.md`
- `config/codeatlas.example.toml`, `config/dependency-lock.json`
- `tests/acceptance-scenarios.json`
- `scripts/validate_kit.py`

## Decisions and ADRs

ADR-0012 records generation-independent explicit notes, exact evidence identity,
trusted process-level mutation policy, strict resource URIs and static client prompt
templates. No new third-party dependency was introduced. Memory rows remain
plaintext local application data and are explicitly not authoritative code facts.

## Commands and evidence

| Command | Exit code | Result | Log / artifact |
|---|---:|---|---|
| `git status --short` | 128 | directory is not a Git worktree | terminal diagnostic |
| pinned `cargo run -p xtask --locked --offline -- verify` before edits | 0 | Phase 12 prerequisite: 93 tests passed, 2 helper entry points ignored | terminal output |
| first `cargo check -p ca-storage --locked --offline` | 101 | caught a `u64` SQLite binding; converted revisions to checked `i64` | terminal diagnostic |
| `cargo check --workspace --locked --offline` | 0 | schema/tool/resource/prompt integration compiled | terminal output |
| focused storage memory test | 0 | revision, evidence, stale filtering, GC and deletion passed | terminal output |
| `cargo test -p ca-mcp -p ca-cli --tests --no-run --locked --offline` | 0 | MCP/CLI test targets compiled | terminal output |
| focused Phase 13 modern/legacy lifecycle tests | 0 | two real-binary protocol-era tests passed | terminal output |
| one attempted `cargo test` with two positional filters | 1 | Cargo rejected the second filter; rerun as the complete lifecycle target | terminal diagnostic |
| `cargo test -p ca-cli --test lifecycle --locked --offline` | 0 | 15 lifecycle tests passed | terminal output |
| first strict workspace clippy | 101 | one collapsible-if style diagnostic; fixed without policy change | terminal diagnostic |
| `cargo fmt --all` plus strict workspace clippy | 0 | formatting and warnings clean | terminal output |
| `cargo test --workspace --locked --offline` | 0 | 97 tests passed, 2 helper entry points ignored | terminal output |
| strict clippy after the final response-budget change | 101 | caught an unsupported `Serialize` bound on rmcp's response wrapper; the check now measures the serializable inner result before wrapping | terminal diagnostic |
| final `cargo run -p xtask --locked --offline -- verify` | 0 | fmt/clippy/tests/fixtures pass; 99 tests passed, 2 ignored | terminal output |
| `python3 scripts/validate_kit.py` | 0 | 18 prompts, 43 scenarios (33 pass, 2 partial, 8 not executed), Markdown/link checks pass | terminal output |
| `jq empty config/dependency-lock.json tests/acceptance-scenarios.json` | 0 | machine-readable evidence parses | terminal output |

## Acceptance criteria

| Criterion | Passed / failed / not run | Evidence |
|---|---|---|
| All sixteen tools implemented with truthful policy surface | passed | default 14-tool and opted-in 16-tool lists; generated closed schemas and exact annotations in both eras |
| Notes remain distinct from authoritative code facts | passed | provenance and `authoritative_code_fact=false` in tool/resource results; no memory-to-symbol fact insertion |
| Trusted opt-in, revisions, size/count/policy checks | passed | hidden default mutations, backend authorization, conflict/evidence/oversize tests and writer-owner enforcement |
| Reindex/GC/restart/migration/backup preserve notes and surface stale evidence | passed | storage and subprocess retention tests plus existing migration/SQLite-backup coverage |
| Resources preserve authorization/freshness/budgets | passed | typed parser attack tests and delegation to repository-bound status/map/symbol/memory backends |
| Prompt templates work without an internal LLM | passed | modern/legacy prompts/list/get and synthetic injection-as-data test |

## Known limitations and risks

- Only macOS arm64 executed. Linux and Windows native SQLite/process/path behavior
  remains for configured CI; this non-Git directory cannot dispatch it.
- No actual installed-Codex prompt/resource UI session was run. Protocol exposure is
  proven through independent stdio clients, not a claim about slash-command UX.
- Static prompt boundaries cannot guarantee a remote model resists every injection.
  Memory/source text can leave the device through the selected MCP client.
- Search is lexical FTS over explicit note text; there is no embedding, automatic
  chat import, synthesized memory, global user memory or automatic note write.
- Large-corpus memory latency/storage growth, parser fuzzing, power-loss durability,
  `cargo audit` and `cargo deny` remain Phase 14 work or unavailable local tooling.

## Blockers and safe next actions

No Phase 13 blocker remains on the available host. Phase 14 may run hardening,
fuzzing, adversarial resource/output tests and measured performance without changing
the trusted memory-write boundary or treating prompt wording as a security sandbox.

## Exact next prompt

`prompts/14-hardening-and-evaluation.md`
