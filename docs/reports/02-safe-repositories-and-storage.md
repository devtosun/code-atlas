# Phase 02 report — authorized paths, worktrees and SQLite generations

State: completed
Date: 2026-09-19
Git revision and local diff: unavailable — the supplied directory is not a Git
worktree; `git status --short --branch` exits 128. Existing files were preserved and
no commit, push, release, user configuration or Codex installation was performed.
Host / target / toolchain: macOS 27.0 (26A428), arm64 /
`aarch64-apple-darwin`, Rust 1.98.1, Cargo 1.98.1, LLVM 22.1.8

## Scope implemented

- Canonical authorized roots, bounded forward-slash relative source paths,
  component-aware containment, link/reparse-point refusal, final-handle identity
  checks, file/scan bounds, UTF-8/binary diagnostics and deterministic scans.
- `.gitignore`, `.ignore` and `.codeatlasignore` matching with hard secret, build,
  minified and non-source exclusions that repository rules cannot override.
- Isolated, strict `gix` worktree metadata reads; Gitless, main-worktree and linked-
  worktree identities; BLAKE3 IDs include the canonical root and Git directories.
- Platform application-data paths outside source roots, visible without mutation in
  `doctor`; owner-only 0700/0600 policy on Unix and inherited current-user ACL policy
  on Windows.
- SQLite schema v2 for generations, immutable file versions, generic test facts,
  external-content FTS search rows, jobs and durable memory/evidence scaffolding.
- Runtime SQLite >=3.51.3 and FTS5 probes, foreign keys, WAL, `synchronous=FULL`,
  transactional migrations and refusal of foreign, inconsistent or future schemas.
- A nonblocking `fs4` owner lock held by the dedicated SQLite writer thread, a
  bounded mutation queue/busy timeout, query-only followers and explicit
  `WRITER_BUSY` mutation errors.
- Atomic generation activation, generation-pinned reader transactions, interrupted-
  stage recovery, literal FTS query quoting and WAL-safe SQLite backup API use.

No parser, language extraction, source-code execution, startup scan, destructive
repair, new MCP search/index tool or automatic memory write was introduced.

## Files changed

- Root `Cargo.toml` / `Cargo.lock` and crate manifests: exact Phase 02 production
  dependencies and the regenerated application lock.
- `crates/ca-core/src/lib.rs`: canonical root authorization.
- `crates/ca-engine/src/repository.rs` and module export: worktree identity,
  SourceReader/FileScanner, policy, diagnostics and adversarial fixtures.
- `crates/ca-storage/src/schema.rs`, `storage.rs` and exports: migrations, paths,
  lock/writer ownership, generations, snapshots, FTS, recovery and backup.
- `crates/ca-cli/src/main.rs` and CLI/workspace tests: truthful Phase 02 `doctor`
  identity/path facts while keeping storage unopened and lifecycle claims unchanged.
- Architecture, data model, dependency/security policy, project state, test matrix,
  dependency lock manifest, ADR-0001 and this report.

## Decisions and ADRs

ADR-0001 remains accepted. Phase 02 implements its previously selected one-owner,
read-only-follower SQLite lifecycle without adding a daemon or network port. No new
architectural exception was needed.

The storage schema deliberately accepts generic test facts rather than fabricated
language extraction. A complete scan is required for activation; a staged or failed
generation never changes the active pointer. Persistent notes are generation-
independent and the v1-to-v2 migration preserves and indexes them.

## Commands and evidence

Commands used the temporary Phase 00 toolchain locations
`RUSTUP_HOME=/private/tmp/codeatlas-rustup` and
`CARGO_HOME=/private/tmp/codeatlas-cargo`; user PATH and configuration were not
changed. Workspace commands ran from the repository root unless noted.

| Command | Exit code | Result | Log / artifact |
|---|---:|---|---|
| `git status --short --branch` | 128 | supplied directory is not a Git worktree | terminal diagnostic |
| `cargo run -p xtask -- verify` before Phase 02 edits | 0 | Phase 01 prerequisite gates reran; 21 tests passed | terminal output; Phase 01 report |
| `cargo generate-lockfile` | 0 | locked 257 Rust-1.98.1-compatible packages | production `Cargo.lock` |
| `cargo test -p ca-engine -p ca-storage --locked` | 0 | 17 focused repository/storage tests passed; one ignored helper was invoked successfully as a child process | terminal output |
| `cargo run -p ca-cli --locked -- doctor --json --root <workspace>` | 0 | canonical gitless ID and external data/DB paths reported; DB remained absent | terminal JSON |
| `cargo fmt --all -- --check` | 0 | final formatting gate passed | terminal output |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | final lint gate passed | terminal output |
| `cargo test --workspace --locked` | 0 | 37 tests passed and one helper was ignored at the outer harness, including both MCP lifecycle eras and 30-start regression | terminal output |
| `cargo run -p xtask -- verify` | 0 | orchestrated format, clippy and locked-test gates passed | terminal output |
| `python3 scripts/validate_kit.py` | 0 | kit structure, references and phase state validated | terminal output |
| `cargo audit --version`; `cargo deny --version` | 101 each | tools are not installed; no audit result claimed | exact Cargo diagnostics |
| `rustup target list --installed` | 0 | only `aarch64-apple-darwin` is installed | terminal output |

Initial dependency resolution inside the network-restricted sandbox failed with DNS
errors. The lock/download commands were rerun with explicit approval; this was
build-time access only. The runtime source/storage paths contain no network client.

## Acceptance criteria

| Criterion | Passed / failed / not run | Evidence |
|---|---|---|
| Unauthorized/out-of-root files are not returned by scanner/reader | passed on macOS arm64 | traversal, repo/repo2 prefix, ignore, secret/non-source, symlink and final-component replacement tests |
| Distinct worktrees and Gitless roots get local distinct databases | passed | Gitless/main/linked identity test plus distinct StoragePaths assertion |
| Readers see one committed generation; failed activation retains old | passed | pinned old-reader, incomplete activation and new-reader transaction test |
| One writer, safe followers and discovery independence | passed on macOS arm64 | actual OS lock contention, follower read/`WRITER_BUSY`, owner handoff and unchanged MCP lifecycle tests |
| Real SQLite/FTS version, migration and backup | passed | SQLite 3.53.2, FTS insert/update/delete, v1→v2 note preservation, integrity-checked backup |
| Recovery/error behavior is explicit and non-destructive | passed | abandoned/interrupted reopen state, corrupt/future/foreign metadata refusal, read-only backup permission error |
| Native Linux path/lock behavior | not run | no Linux target/host installed |
| Native Windows junction/ACL/lock behavior | not run | no Windows target/host installed; configured CI remains undispatched |

## Known limitations and risks

- The available host is macOS arm64 only. Windows junction/reparse behavior and ACLs,
  and Linux path/lock behavior require native CI; cross-compilation would not count.
- Portable `std` traversal has a documented intermediate-directory enumeration race
  against a hostile same-user process. Returned source bytes still pass component,
  no-follow final-open, file-identity and canonical-containment checks. Stronger
  hostile-co-tenant guarantees would need platform handle traversal.
- The host refused creation of an invalid-UTF-8 filename fixture with `EPERM`; the
  native `OsString` rejection path is tested, but an on-disk invalid-name scan awaits
  a supporting native filesystem/runner.
- Interrupted staging recovery is tested from durable building state after writer
  shutdown. It is not a power-loss proof, filesystem durability certification or
  kill-at-every-instruction campaign.
- The owner is reacquired by reopening `Storage`; automatic in-place follower
  promotion is not implemented. No watcher exists yet.
- `cargo audit` and `cargo deny` were unavailable and were not installed implicitly.

## Blockers and safe next actions

No Phase 02 blocker on the available host. The safe next implementation step is the
parser kernel; native Linux/Windows CI should be run when the project is placed in a
Git worktree and the configured workflow can execute.

## Exact next prompt

`prompts/03-parser-kernel.md`
