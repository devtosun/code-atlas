# Deferred native qualification — Linux x64 and Windows x64 MSVC

$ca-phase-driver $ca-release-validation $ca-security-review $ca-mcp-contract

## Task and scope

Qualify Linux x64 and Windows x64 MSVC as supported CodeAtlas release targets after
the macOS ARM64 Phase 14/15 path is complete. Run on native hosted runners or native
machines; cross-compilation alone is not execution evidence. Do not alter the
already-qualified macOS support claim unless a shared-code regression is found.

## Prerequisites

- Read `AGENTS.md`, `docs/PROJECT_STATE.md`, `docs/TEST_MATRIX.md`,
  `docs/SECURITY_PRIVACY.md`, `docs/DEPENDENCY_POLICY.md`, the Phase 14 report and
  the Phase 15 report if it exists.
- Preserve the pinned Rust toolchain, `Cargo.lock`, protocol revisions and grammar
  versions. Record any unavoidable platform-only dependency change explicitly.
- Confirm that the target runner is genuinely native and record its OS image,
  architecture, filesystem, compiler/linker and Rust versions.

## Linux x64 matrix

1. Build the locked release on a clean x86_64 Linux runner and record binary hash,
   size, dynamic libraries, glibc/musl requirement and symbol-stripping result.
2. Run formatting, strict clippy, workspace tests, `xtask verify`, kit validation,
   advisory/license policy checks and all three isolated fuzz targets.
3. Run modern and legacy stdio lifecycle, tools/resources boundary attacks, writer
   termination, corrupt/migration-failure recovery, read-only directory, lock
   contention, watcher overflow/reconciliation, cancellation and shutdown cases.
4. Recreate the Phase 14 accuracy and 10k/100k performance artifacts. Keep raw
   timings and limitations; do not substitute macOS numbers.
5. Extract the Phase 15 archive into clean paths containing spaces and Unicode,
   then run doctor, fixture indexing and protocol smoke without language SDKs,
   a daemon, a model download or network access at runtime.

## Windows x64 MSVC matrix

1. Build the locked release with the pinned MSVC target on a clean native Windows
   runner. Record PE hash, size, imported DLLs, VC runtime assumptions and stripping.
2. Run the same quality, protocol, fuzz, accuracy and performance gates natively.
3. Add/execute Windows-specific cases for drive letters, UNC/device paths, reserved
   names, case-insensitive collisions, long paths, symlink and junction escapes,
   reparse points, ACL-denied/read-only directories, atomic replacement semantics,
   file sharing, writer lock release after process death and watcher rename bursts.
4. Extract the Phase 15 archive under paths with spaces and non-ASCII characters;
   run doctor, fixture index/search/read/context, restart and follower smoke without
   .NET, Java, Node, Go or Dart SDKs.
5. Dry-run/apply/remove the owned Codex entry against a disposable config fixture.
   Prove unrelated TOML comments and MCP entries remain byte-for-byte unchanged,
   `required = false`, backups are created and removal preserves source/notes.

## Evidence and acceptance

- Store target-specific machine-readable artifacts under
  `docs/reports/artifacts/` and never overwrite another platform's results.
- Update `docs/TEST_MATRIX.md`, `docs/PROJECT_STATE.md`, support/limitations docs,
  Phase 14 evidence and the Phase 15 release report/checksums.
- A target is supported only when its native correctness/security, recovery,
  performance/accuracy and extracted-package gates pass. Otherwise leave it
  `NOT RUN` or `BLOCKED` with the exact failing command and diagnostic.
- Do not publish, sign, notarize, modify a real user Codex configuration or change
  system PATH/ExecutionPolicy without separate explicit authorization.

## Handoff

Report commands and exit codes, runner identities, package hashes, measured results,
failures and residual risks. If both matrices pass, change the support matrix from
macOS-only to the exact tested Linux/Windows targets. If either fails, keep that
target unsupported and provide the smallest follow-up repair prompt.
