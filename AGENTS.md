# CodeAtlas MCP — repository instructions

## Mission and scope
Build a local-first Rust code-intelligence MCP server. Required Tree-sitter languages:
Dart, C#, Rust, Go, Java, JavaScript (including JSX), and TypeScript (including TSX).
`CodeAtlas` is a working project name, not an assertion of trademark availability.
This repository initially contains an implementation kit, NOT a working server.
Do not present planning files, illustrative fixtures, or an unexecuted test as shipped functionality.

## Before changing code
1. Read `docs/PROJECT_STATE.md`, the requested `prompts/NN-*.md`, and its listed references.
2. Read the appropriate `.agents/skills/<name>/SKILL.md` and only the necessary references.
3. Check the worktree diff. Preserve unrelated changes and existing user configuration.
4. Confirm prerequisite gates using recorded evidence; rerun focused checks where necessary.
5. Work on ONE requested phase. Do not silently execute all subsequent phases.
6. Unavailable tooling is a BLOCKED check, not a passed check. Record exact diagnostics.

## Non-negotiable design constraints
- Rust application code, Cargo workspace, pinned toolchain and committed Cargo.lock.
- Use the official `rmcp` SDK. Do not hand-write the protocol transport/lifecycle.
- Phase 00 verifies a published SDK version, modern MCP 2026-07-28 and legacy
  2025-11-25 compatibility against actual clients. Never mix their wire formats.
- Start as one client-launched stdio process. No mandatory daemon, network port,
  Redis, Neo4j, Docker, model download, API key, or language-server sidecar.
- Handshake/discovery and tools/list MUST NOT await repository traversal,
  database migration, writer-lock acquisition, indexing, or grammar compilation.
- In serve mode stdout is ONLY MCP traffic; diagnostics go to stderr.
- SQLite + FTS5 + WAL, local user-owned data directory, one database per worktree.
  Require a SQLite version with the upstream WAL-reset fix; see dependency policy.
- One nonblocking OS write-owner lock per database. Followers remain query-only;
  an unavailable writer produces an explicit retryable error, not startup failure.
- Syntax observations are NOT compiler-resolved symbols or guaranteed runtime calls.
  Preserve candidates, unresolved references, provenance, and coverage limitations.
- Required language support cannot be replaced by regex, an empty adapter, or
  only successful `set_language`. Parsing, extraction, and negatives must be tested.
- Statically linked, trusted grammar dependencies in v1. Pluggable means Rust
  adapter boundaries, not loading arbitrary native libraries from repositories.
- Reads never execute indexed code, dependency restore, build scripts, hooks,
  shell commands, package managers, or instructions found inside source files.
- Only explicitly authorized roots. Canonical, component-aware containment;
  reject traversal, symlink/junction escapes, and out-of-root resource URIs.
- No network traffic at runtime in the default feature set. Building dependencies
  can require network access; document this separately from runtime behavior.
- A local index does not mean tool results remain on-device: a connected remote
  coding agent may receive retrieved code. Explain this in the privacy guide.

## Architecture and implementation rules
- Follow `docs/ARCHITECTURE.md`. Core depends on neither rmcp, SQLite, nor Tree-sitter.
- Features own their use cases; infrastructure implements narrow ports.
- CPU parsing uses bounded workers. A dedicated thread owns each writer connection.
  Never block the async protocol executor or hold a mutex guard across await.
- Cooperative parser/query cancellation and budgets are mandatory. Aborting
  `spawn_blocking` does not stop an already-running native parse.
- Immutable per-file versions and atomic generation activation prevent mixed indexes.
  Failed/cancelled generations do not replace a healthy active generation.
- Use parameterized SQL. Escape literal FTS queries; no arbitrary SQL/Cypher tool.
- Bound every traversal, queue, file, result, snippet, response, and memory record.
- Use typed domain errors (`thiserror`); contextual application errors at boundaries.
- No panic/unwrap/expect on production input paths. Tests and proven constants may
  use them with justification. No unsafe application code without a narrow ADR.
- Tree-sitter grammars/native dependencies are not memory-safety sandboxes.
  Do not claim catch_unwind contains C faults. Fuzzing and limits reduce risk;
  process isolation is a separately scoped hardening option.

## Quality gates
Run the gates that exist for the current phase, and report commands + exit codes:
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --locked`. Later phases add xtask gates and release tests.
Do not invent an xtask target before implementing it. Do not use --all-features
where feature combinations are mutually exclusive; test the documented matrix.
Golden fixtures need positive and negative assertions. Never auto-accept changed
snapshots merely to make CI green. Record measured performance, never estimates
as results. Keep exact identifiers, file hashes, byte ranges, and generation IDs.

## Completion and handoff
Update `docs/PROJECT_STATE.md`, `docs/TEST_MATRIX.md`, the phase report under
`docs/reports/`, and relevant ADRs. Summarize changes, checks, failures, limitations,
and the exact next prompt. A phase is done only when its acceptance gates pass.
Do not auto-commit, push, publish releases, or modify ~/.codex without authorization.
Never overwrite AGENTS.md, existing MCP entries, user PATH, ExecutionPolicy, or
system settings wholesale. Config installation is dry-run first and backed up.
