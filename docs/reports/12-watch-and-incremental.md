# Phase 12 report — watcher, incremental reconciliation and worktrees

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable; the supplied directory is not a Git
worktree (`git status --short` exits 128)
Host / target / toolchain: macOS 27.0 (26A428), arm64 / aarch64-apple-darwin /
rustc 1.98.1 (48a229cea), LLVM 22.1.8

## Scope implemented

- Added trusted, opt-in `serve --watch` lifecycle. Discovery/status before an index
  still performs no storage open or traversal; watching starts only after this
  process acquires and retains write ownership.
- Added notify 8.2.0 native watching plus an explicit/content-comparing PollWatcher
  path and automatic initialization fallback. Both observe only the authorized root
  with symlink following disabled.
- Added a bounded callback queue, nonblocking overflow accounting, debounce/coalescing
  and independent periodic reconciliation. Events are hints; every run repeats the
  ignore-aware scan and content hashes.
- Serialized manual and watch jobs through the existing durable indexing pipeline.
  Changed files receive full parsing and every candidate receives full graph
  re-resolution before atomic activation. No Tree-sitter old-tree reuse was added.
- Extended `repository_status` with enabled/running/backend, pending/reconciling,
  queue/event/coalescing/overflow/reconciliation counters, last trigger/time/error,
  and explicit degraded source-filesystem guarantees.
- Retained one owner storage handle for the stdio session. Followers remain useful
  for reads, receive retryable writer contention on mutations, and can acquire the
  lock after clean shutdown or process death.
- Shutdown cancels active jobs, wakes and joins the watcher, joins index workers and
  releases ownership. No daemon, network port, system watch-limit change or user
  configuration mutation was introduced.

## Files changed

- `Cargo.toml`, `Cargo.lock`, `crates/ca-cli/Cargo.toml`
- `crates/ca-cli/src/main.rs`, `crates/ca-cli/src/mcp_backend.rs`
- `crates/ca-cli/src/watcher.rs`
- `crates/ca-cli/tests/cli.rs`, `crates/ca-cli/tests/lifecycle.rs`
- `docs/adr/0011-owner-watch-reconciliation.md`
- `docs/reports/artifacts/12-watch-latency.json`
- `docs/ARCHITECTURE.md`, `docs/DATA_MODEL.md`, `docs/DEPENDENCY_POLICY.md`
- `docs/MCP_CONTRACT.md`, `docs/SECURITY_PRIVACY.md`, `docs/PROJECT_STATE.md`
- `docs/TEST_MATRIX.md`, `README.md`, `README.tr.md`
- `config/dependency-lock.json`, `tests/acceptance-scenarios.json`
- `scripts/validate_kit.py`, `docs/reports/11-mcp-tools.md`

## Decisions and ADRs

ADR-0011 records the owner-scoped lifecycle, bounded hint queue, polling fallback,
periodic correctness scan and shutdown ordering. Phase 12 adds no database migration
or MCP tool. notify is infrastructure in `ca-cli`; engine/domain boundaries remain
free of notify types. The default remains `watch=false` and `auto_index=false`.

## Commands and evidence

| Command | Exit code | Result | Log / artifact |
|---|---:|---|---|
| `git status --short` | 128 | directory is not a Git worktree | terminal diagnostic |
| first `cargo run -p xtask --locked -- verify` without task Cargo/Rustup paths | 127 | `cargo` absent from inherited PATH | terminal diagnostic |
| first pinned-toolchain verify without matching Cargo cache | 130 | attempted unavailable crates.io DNS; retry loop stopped | terminal diagnostic |
| pinned `cargo run -p xtask --locked --offline -- verify` | 0 | Phase 11 prerequisite gates passed: 87 tests passed, 2 helper tests ignored | terminal output |
| escalated pinned `cargo check --workspace` | 0 | resolved/downloaded exact notify 8.2.0 and 18 locked transitive target packages | `Cargo.lock` |
| first native watcher burst test | 101 | FSEvents initialized but no callback observed in five seconds; test revised to verify native startup plus authoritative periodic reconciliation without fabricating event latency | terminal output |
| `cargo test -p ca-cli watcher::tests --locked --offline` | 0 | native initialization/periodic path and simulated overflow/lost-event polling path passed | terminal output |
| focused all-language polling watch lifecycle test | 0 | edit corpus converged and matched clean full counts | `docs/reports/artifacts/12-watch-latency.json` |
| focused owner/follower clean takeover test | 0 | follower rejected while owner alive; later process acquired ownership | terminal output |
| focused killed-owner recovery test | 0 | OS lock released and next full owner activated 201 files | terminal output |
| focused watcher EOF recovery test | 0 | pending/active watcher work stopped; next owner opened and rebuilt 601 files | terminal output |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 0 | no warnings before documentation handoff | terminal output |
| final `cargo run -p xtask --locked --offline -- verify` | 0 | fmt and clippy clean; 93 tests passed, 2 helper entry points ignored; all fixtures passed | terminal output |
| `python3 scripts/validate_kit.py` | 0 | 18 prompts, 43 scenarios (30 pass, 2 partial, 11 not executed), 74 Markdown files/links | terminal output |
| `jq empty config/dependency-lock.json tests/acceptance-scenarios.json docs/reports/artifacts/12-watch-latency.json` | 0 | machine-readable evidence parses | terminal output |
| final `git status --short` | 128 | directory remains outside a Git worktree | terminal diagnostic |

The measured debug fixture used a 50 ms debounce and 1,000 ms reconciliation
interval. Its atomic-save/rename/delete/recreate/case-rename/Unicode/ignore/burst/Git
HEAD sequence converged in 1,137.715625 ms with 10 active files. This is one small
fixture observation, not a 10k-file p95 claim. FSEvents callback latency is not
reported because the managed host did not deliver a callback during the short test.

## Acceptance criteria

| Criterion | Passed / failed / not run | Evidence |
|---|---|---|
| Incremental/full equivalence across required languages | passed | watched nine dialect seeds plus edits; active file/fact/symbol counts equal independent full rebuild |
| Missed/overflowed events eventually reconcile | passed | bounded queue overflow counter, overflow trigger and independent periodic reconciliation tests |
| Only owner watches/writes; follower can later acquire | passed | subprocess contention, clean EOF takeover and killed-owner recovery tests |
| Status responsive and pending work visible | passed | MCP polling during burst plus `pending_reconciliation` and `reconciling` fields |
| Exit/EOF stops watcher work consistently | passed | watcher handle wake/join and 601-file EOF recovery test |
| Native OS tests and measured latency | passed with limitation | macOS FSEvents initialization/periodic test; polling latency artifact; no native callback latency claim |

## Known limitations and risks

- Linux inotify and Windows ReadDirectoryChangesW/junction/case behavior were not run
  locally. Only the configured CI matrix can provide native evidence.
- The macOS FSEvents backend initialized but emitted no callback during the managed
  short-burst fixture. Periodic reconciliation prevents permanent staleness, but
  native event-driven latency on this host remains unmeasured.
- PollWatcher content comparison and periodic full scans can be expensive on large
  or mounted trees. The 10k-file p95 target, idle CPU/RSS and source-mount behavior
  remain Phase 14 measurement work.
- Git HEAD changes are included as hints and paired with worktree content changes;
  the fixture simulates a branch transition rather than invoking a Git checkout.
- Runtime config/grammar upgrades take effect through restart/fingerprint mismatch;
  there is no live trusted-config reload in this phase.
- Physical power-loss durability, parser fuzzing and actual installed-Codex execution
  remain untested. The directory is not a Git worktree, so no revision/diff or CI
  dispatch evidence is available.

## Blockers and safe next actions

No Phase 12 blocker remains on the available macOS arm64 host. Phase 13 may add
explicit memory privileges, resources and server prompts without changing watcher
root authority or treating repository configuration as trusted. Linux/Windows native
watcher execution and the macOS callback-delivery limitation remain recorded rather
than being treated as cross-platform evidence.

## Exact next prompt

`prompts/13-memory-resources-prompts.md`
