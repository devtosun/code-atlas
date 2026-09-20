# Phase 14 report — security, recovery, fuzzing and measurements

State: completed for the supported macOS ARM64 target
Date: 2026-09-20
Git state during evidence capture: newly initialized `main` worktree before its first
commit; this report and its artifacts are included in the containing initial commit
Host / target / toolchain: macOS 27.0 arm64, Apple M2 Pro (10 cores), 16 GB /
aarch64-apple-darwin / rustc 1.98.1 (48a229cea), LLVM 22.1.8

## Supported scope

Phase 14 qualifies macOS ARM64 as the current supported native target. The user
explicitly deferred Linux x64 and Windows x64 MSVC; neither is implied by this pass.
They remain `NOT RUN`, unsupported targets with an executable follow-up in
`prompts/14-linux-windows-native-validation.md`.

## Scope implemented

- Tightened repository-relative paths, 64-character lowercase cursor fingerprints
  and resource URI boundaries. Deterministic generated properties cover paths,
  IDs/ranges, cursors, response reduction, resource URIs and generation joins.
- Added modern and legacy real-stdio adversarial tests for caller roots, traversal,
  absolute/Windows-form/secret/generated paths, resource schemes, malicious FTS,
  oversized requests and inert `build.rs`/`package.json` payloads.
- Strengthened killed-writer recovery to retain the old active generation and an
  explicit note. Forced migration failure proves schema/data rollback.
- Migrated SQLite to schema v6 with generation-membership and folded-name indexes,
  then repaired 100k cleanup/search and targeted watcher generation updates without
  increasing the writer deadline or weakening atomic activation.
- Added three workspace-isolated cargo-fuzz targets, exact development lockfile and
  seed corpora. Each target built and ran for five minutes on an isolated corpus.
- Added reproducible performance, process-capability, dependency and accuracy
  evidence. The accuracy gate now checks exhaustive reviewed fixture declarations
  plus a source-hash-locked five-file corpus from this authorized project.
- Added explicit `deny.toml`; cargo-audit and cargo-deny run against the locked graph.
- Enabled Cargo release-profile symbol stripping and rebuilt the measured binary.

## Principal files changed

- Production and validation: `Cargo.toml`, `deny.toml`, `xtask/src/main.rs`,
  `scripts/phase14_*.py`, `scripts/validate_kit.py`
- Fuzzing: `fuzz/Cargo.toml`, `fuzz/Cargo.lock`, `fuzz/README.md`,
  `fuzz/corpus/**`, `fuzz/fuzz_targets/**`
- Accuracy: `fixtures/phase14-declaration-precision.json`,
  `fixtures/phase14-real-corpus.json`, `fixtures/phase14-real-corpus.golden`
- Evidence: `docs/reports/artifacts/14-*.json`
- State/policy: `docs/PROJECT_STATE.md`, `docs/TEST_MATRIX.md`,
  `docs/SECURITY_PRIVACY.md`, `docs/DEPENDENCY_POLICY.md`,
  `docs/ARCHITECTURE.md`, `config/dependency-lock.json`, `README.md`, `README.tr.md`
- Deferred work: `prompts/14-linux-windows-native-validation.md`

The earlier Phase 14 implementation also changed the core/engine/storage/MCP/CLI
code and tests recorded in this repository. No user Codex configuration, global PATH,
system setting, release publication or signing state was changed.

## Decisions

Linux/Windows were not relabelled as passing. Instead, the supported release scope is
macOS ARM64 until native execution proves those targets. Fuzzing remains a separate
nightly development package; no nightly component enters the stable production
workspace. Duplicate dependencies are warnings for review, while advisories, yanked
packages, wildcard dependencies, unknown registries/Git sources and unreviewed
licenses are policy failures.

The checked-in parser golden was reviewed as an exhaustive declaration label set:
the gate compares every emitted kind, spelling and byte range, rather than counting
only required anchors. The additional real corpus consists only of project-owned
source, records SHA-256 provenance and never uploads source. It remains a small Rust
sample, not a representative multi-repository or compiler-semantic benchmark.

## Commands and evidence

| Command | Exit | Result / artifact |
|---|---:|---|
| focused Phase 14 core/engine/MCP/storage and dual-era subprocess tests | 0 | path/budget properties, attacks, recovery, migration rollback, watcher and cancellation cases pass |
| `cargo +nightly fuzz build` | 0 | three libFuzzer targets build with isolated nightly |
| three `cargo +nightly fuzz run … -max_total_time=300 -timeout=10` runs | 0 / 0 / 0 | 50,075 / 8,004,978 / 8,630,950 units; zero crash artifacts/timeouts; `14-fuzz-results.json` |
| `cargo audit --no-fetch --deny warnings --json` | 0 | 294 dependencies, 1,251 advisories, zero vulnerabilities/warnings |
| `cargo deny --locked --offline --format json check` | 0 | advisories, bans, licenses and sources pass; duplicate-version warnings retained |
| `cargo build --release --locked --offline -p ca-cli` | 0 | stripped macOS ARM64 release built |
| `python3 scripts/phase14_measure.py … --file-counts 10000 100000` | 0 | final stripped-binary timing/RSS artifact |
| `python3 scripts/phase14_process_audit.py …` | 0 | 14 default tools, runtime networking false, no `lsof -i` rows |
| `cargo run -p xtask --locked --offline -- verify` | 0 | fmt, strict clippy, 112 tests (2 helpers ignored), all fixture goldens and real-corpus gate pass |
| `python3 scripts/phase14_accuracy.py … --verified-fixture-gates --verified-real-corpus-gate` | 0 | exact declaration/reference/capacity artifact generated |
| `python3 scripts/phase14_dependency_audit.py …` | 0 | locked metadata plus actual audit/deny scan artifact generated |
| `python3 scripts/validate_kit.py` | 0 | repository structure, prompt/skill references, TOML, fixtures and state validate |
| full repository `codeatlas index --full --json` diagnostic | 1 | one repository source exceeded the per-file fact/search-document bound; no active generation was displaced |

## Release binary and performance

Binary SHA-256:
`ec90f1123317a56c3a054745c0f27b08b4240cb7c632742ceb02249640ff751e`.
The stripped Mach-O is 24,791,408 bytes. `otool -L` reports CoreFoundation,
CoreServices, libiconv and libSystem only. Measurement used APFS-backed temporary
storage with uncontrolled page cache, power mode and background load. RSS was sampled
with `ps` at roughly 5 ms intervals and can miss shorter peaks.

| Measurement | Result | Disposition |
|---|---:|---|
| modern spawn through tools/list, 30 samples | p95 13.700 ms; max 393.564 ms | pass: p95 <= 1 s; max <= 3 s |
| legacy spawn through tools/list, 30 samples | p95 6.910 ms; max 6.964 ms | pass |
| 10k full index | 9.994 s; 1,000.631 files/s; 97,320,960-byte peak RSS | baseline |
| 100k full index | 110.237 s; 907.137 files/s; 535,855,104-byte peak RSS | baseline; 100k files/facts/symbols |
| 10k no-op / one-file CLI update | 0.521 s / 0.533 s | 10k reused / one parsed |
| 100k no-op / one-file CLI update | 11.389 s / 10.597 s | full safety scan retained |
| 10k watcher edit, 10 samples | post-debounce p95 214 ms; event-to-activation p95 2,326.930 ms | pass: <= 500 ms after debounce; polling adds up to 2 s |
| 100k warm exact lookup, 50 samples | p95 60.323 ms; max 185.434 ms | pass: p95 <= 100 ms |
| status during active index, 20 samples | p95 0.143 ms | pass |
| active-job cancellation | ack 179.275 ms; terminal cancelled 385.353 ms | pass; active work observed |
| shutdown after work | 19.597 ms | pass |

## Accuracy evidence

Required declaration-anchor recall is 157/157 across Rust, Go, C#, Java, Dart,
JavaScript, JSX, TypeScript and TSX. Twenty-three explicit forbidden-name checks
observe zero unexpected rows. The reviewed exhaustive parser golden labels 143/143
emitted fixture declarations as correct across the nine dialects. The separately
labelled resolution corpus covers 35 sites: syntax coverage is 5/5 per language and
supported binding precision/recall remains C# 6/6, Dart 3/3, Go 4/4, Java 5/5,
JavaScript 3/3, Rust 4/4 and TypeScript 4/4.

The authorized larger sample contains five real CodeAtlas Rust files, 6,083 bytes
and 78/78 reviewed declarations. SHA-256 and parser source hashes bind labels to the
exact source. The generated 100k Rust corpus reports 100k active files/facts/symbols
and zero failures; it is capacity evidence, not a real-world accuracy score.

## Acceptance criteria

| Criterion | Status | Evidence |
|---|---|---|
| Required correctness/security cases pass on supported native targets | passed | macOS ARM64 workspace, subprocess and adversarial gates |
| No active-index corruption or note loss in recovery tests | passed | generation isolation, killed writer and migration rollback |
| No demonstrated root escape or unauthorized execution | passed | dual-era tool/resource attacks and inert payloads |
| Budget/cancellation observes bounded real work | passed | generated reducers, caps and active cancellation timing |
| Accuracy/performance contain real counts/times/limitations | passed | accuracy, performance, process and fuzz artifacts |
| Dependency/license/advisory checks complete | passed | audit/deny and metadata inventory |
| Fuzz targets execute without observed crashes | passed with stated limitation | three five-minute runs; not a security proof |
| Lookup and watcher edit targets | passed | 60.323 ms and 214 ms p95 |
| Linux x64 / Windows x64 MSVC | not run / unsupported | explicitly deferred; no cross-platform claim |

## Known limitations and risks

- Linux x64 and Windows x64 MSVC are not supported until their native qualification
  prompt passes. Windows junction/ACL/reparse and both platforms' watcher/lock/package
  behaviors remain unexecuted.
- Five-minute fuzz runs can miss defects. Native grammar C code is trusted and not a
  memory-safety sandbox; symbolizer/atos startup warnings were observed.
- The accuracy corpus is curated and small. Syntax observations are not compiler or
  runtime semantics, and no representative external multi-repository score is claimed.
- Indexing this entire development repository currently hits the bounded per-file
  fact/search-document limit. The error is explicit and leaves the active generation
  intact, but Phase 15 release notes must expose this large-file limitation.
- Targeted edits still perform full-generation resolution; dense graphs remain a
  scaling risk. Polling fallback adds detection latency before the measured debounce.
- Repository reads retain the documented hostile same-user TOCTOU residual. Normal
  source can contain secrets, and a remote MCP client/model can receive retrieved code.
- Signing, notarization, packaging, installation and real Codex UI smoke are Phase 15
  work and are not claimed here.

## Handoff

The supported macOS ARM64 Phase 14 gates are complete. Proceed only with
`prompts/15-release-and-codex.md`, scoped to macOS ARM64. Do not publish or modify
user Codex configuration without explicit authorization. Run
`prompts/14-linux-windows-native-validation.md` later to add those targets.

## Exact next prompt

`prompts/15-release-and-codex.md`
