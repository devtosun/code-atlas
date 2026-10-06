# Code review repairs — CR-001–CR-013

Date: 2026-10-06. Target: native `aarch64-apple-darwin`, macOS 27.0 ARM64.
Request: implement the confirmed findings in
[`CODE_REVIEW_REPORT.md`](../reviews/CODE_REVIEW_REPORT.md). This is repair work,
not a new numbered phase or approval to publish. The original review is preserved.

## Outcome and provenance

All thirteen confirmed findings have implementation changes and passing regression
evidence on freshly compiled source. This closes those findings, not every suspected
issue in section C of the review and not a compiler-semantic or production-safety
certification. Linux/Windows remain deferred and unsupported.

Working tree: `main`, HEAD `8461837e5cff3b002e8b8ffff5b5cd9793e4ba63`, deliberately
dirty. Existing Phase 15 source, packaging and documentation changes were preserved.
No commit, push, release publication, real Codex configuration mutation or optional
phase was performed. No private repository was used as a test corpus.

The review's missing-toolchain blocker was resolved using a checksum-verified
official rustup bootstrap and isolated Rust 1.98.1/rustfmt/Clippy installation under
`/private/tmp/codeatlas-fix.TY5KJC`. Dependency fetching needed approved build-time
network access; verification used the committed lock offline. User PATH/profile and
system toolchain configuration were not changed. This does not add runtime network
access. The initial dirty source was copied to an isolated baseline: its workspace
tests passed (116 tests plus two parent-exercised subprocess helpers).

The final repaired workspace passes 130 tests plus those two helpers. Rust is
`1.98.1 (48a229cea 2026-09-01)`, Cargo 1.98.1; native compilation uses the installed
Xcode toolchain/SDK. Codebase-memory discovery tools were unavailable; direct source
inspection was the documented fallback. Architecture, storage, resolution, language,
MCP/security and release-validation skills guided the boundaries and negative tests.

## Finding-to-repair evidence

| Finding | Implementation | Passing negative / positive evidence |
|---|---|---|
| CR-001 | Strict non-file parent containment; deterministic equal-range handling; visited/depth/cancellation guards in `scope_chain`; resolver version v2 | LF-less `fn main(){target();}` index terminates; equal-range scope unit and cancelled traversal return promptly; existing EOF/cancel lifecycle tests pass |
| CR-002 | Invalid UTF-8, binary and oversized existing source make full/targeted scans incomplete instead of a deletion | Exact healthy active generation survives all three failures; full/incremental CLI and targeted index tests; actual deletion succeeds |
| CR-003 | Pinned-grammar member selectors retain receiver evidence; references, not just calls, use conservative uncertainty rules | Rust/Go/C#/Java/Dart property selectors cannot become certain local-variable targets; bare-name controls still resolve; receiver extraction negatives |
| CR-004 | Go same-package bare-name binding also requires the same repository-relative parent directory | Same-package sibling files in one directory resolve; identically named packages in different directories do not |
| CR-005 | Imports require an accessible lexical owner; nearest import scope wins before module binding | Function-local alias does not escape to a sibling; nested alias resolves to inner module only inside the block, outer module before/after |
| CR-006 | Schema v7 complete analysis key includes language, grammar, query, extractor and configuration; cache-hit facts stay immutable | Same-content grammar/query/extractor/config changes create distinct versions; repeat key does not append changed payload; pinned prior facts unchanged; v6 migration preserves IDs/FKs/notes; forced migration failure rolls back |
| CR-007 | Bounded result channel; source-capacity chunks are drained/persisted before the next chunk; factory setup before launch; close endpoints and join every worker on failure | 65-file instrumented run has parsed-but-unpersisted high-water ≤5 for capacity 2/batch 3; factory failure produces zero parse results; both worker panics are joined; injected SQLite failure on 129 files preserves active generation and retry completes |
| CR-008 | FK-safe GC detaches expired parents, retains active/immediate predecessor/building candidate and collects orphan file versions | 100 unit-test activations plus 30 changed/no-change CLI generations remain bounded; notes, FK checks and already-pinned WAL snapshot survive; existing stale-generation cursor tests pass |
| CR-009 | Dart combinators extracted from actual grammar nodes and applied to imports and re-exports | Allowed imports and prefixed imports resolve; hidden names do not; repeated `show` filters intersect and `hide` subtracts |
| CR-010 | ECMAScript declaration wrapper recognizes exported arrow/function-expression bindings; local/renamed/default export map resolves correctly | JS/JSX/TS/TSX exported arrows, expressions, renamed local and default imports resolve; non-exported controls and existing type/value fixtures remain intact |
| CR-011 | SQL tuple keysets before per-page limits; disjoint tiers; same-name FTS score dedup; file-version/spelling index | All 11,000 distinct symbols traversed in 275 pages with no repeats and terminal `truncated=false`; storage test pages all five name/FTS tiers and qualified tier with seven-row limits |
| CR-012 | Official rmcp codec/transport, 1 MiB ingress and 128-byte serialized-ID policy; prompt UTF-8 limits and exact result fitting; final typed SDK sink bounds complete JSON line | Both eras reject oversized prompt/scope and escaped prompts; Unicode/control IDs at boundary echo correctly; long IDs, >1 MiB ingress and oversized SDK routing errors cannot emit oversized frames or leave a waiting client |
| CR-013 | Malformed TOML errors contain path/line/column only; parser source and error chain are discarded | Synthetic private canary absent from Display/Debug/source chain and real CLI stdout/stderr; apply/remove × dry-run/apply preserve malformed config bytes; ordinary owned config round-trip passes |

The production changes are in `ca-engine`, `ca-languages`, `ca-storage`, `ca-mcp`
and the pre-existing Phase 15 `ca-cli` Codex integration implementation. Core keeps
its protocol/storage/parser independence. Trusted grammar dependencies and SDK
versions were not upgraded. `futures 0.3.34` and `tokio-util 0.7.19` were promoted
from existing locked dependencies for SDK-compatible sink/stream guards; `rt`
enables the cancellation token. No new registry package version was introduced.

## Reviewed fixture corrections

Changes to `rust-go.golden`, `js-ts.golden`, `csharp-java.golden` and `dart.golden`
were compared with original source and the pinned grammar node types. Source/query
hashes, observation counts, IDs, byte ranges, parse results, scope/call/condition
hashes remain unchanged. Only v2 extractor fingerprints and affected declaration,
reference or import category hashes changed: receiver attributes, transparent export
wrappers and Dart combinators. Parser-kernel and real-corpus goldens are unchanged.

Java `Invoice.java`'s `this.amount` source was independently reviewed as a member
access. Its certainty label changed from `lexically_resolved` to `candidate`, not to
an invented compiler-resolved field. Java candidate-site count is now 4, previously
3; target precision/recall numerators and denominators are unchanged. The complete
35-site label test and prior category fixtures pass. The scanner fixture now expects
an incomplete scan when content limits/encoding errors occur. No snapshots were
blindly accepted, tests deleted, bounds relaxed or timeouts enlarged to hide errors.

During implementation, focused checks caught compile/API mismatches, an overly
broad local-export match and a worker-join path that returned after the first panic.
These were repaired (including joining all handles). Intermediate failures were not
counted as passes; the final gates below were rerun after repair.

## Commands and exit codes

Working directory for all commands is the repository root. `E` below means these
process-local environment assignments, not changes to user configuration:

```text
PATH=/private/tmp/codeatlas-fix.TY5KJC/cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin
RUSTUP_HOME=/private/tmp/codeatlas-fix.TY5KJC/rustup
CARGO_HOME=/private/tmp/codeatlas-fix.TY5KJC/cargo
CARGO_TARGET_DIR=/private/tmp/codeatlas-fix.TY5KJC/target
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
SDKROOT=/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk
CC=/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang
CXX=/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang++
```

| Command (with `E` for Cargo/package commands) | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | final formatting clean |
| `cargo check --workspace --all-targets --locked --offline` | 0 | all targets compile |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 0 | no denied warnings |
| `cargo test --workspace --locked --offline` | 0 | 130 passed; two subprocess-helper entries ignored but exercised by parent tests |
| `cargo test -p ca-storage --locked --offline sql_keysets -- --nocapture` | 0 | all-tier/qualified keysets and FTS dedup |
| `cargo test -p ca-cli --locked --offline bounded_results -- --nocapture` | 0 | bounded results, factory failure and injected worker panic; expected test panic diagnostics, no harness failure |
| `cargo run -p xtask --locked --offline -- grammar-check` | 0 | nine actual pinned providers |
| `cargo run -p xtask --locked --offline -- fixtures` | 0 | 24 parser fixtures |
| `cargo run -p xtask --locked --offline -- rust-go-fixtures` | 0 | eight focused fixtures |
| `cargo run -p xtask --locked --offline -- js-ts-fixtures` | 0 | eleven focused fixtures |
| `cargo run -p xtask --locked --offline -- csharp-java-fixtures` | 0 | eleven focused fixtures |
| `cargo run -p xtask --locked --offline -- dart-fixtures` | 0 | eight focused fixtures |
| `cargo run -p xtask --locked --offline -- phase14-real-corpus` | 0 | five source-hash-locked authorized files |
| `cargo build --release --locked --offline -p ca-cli` | 0 | fresh native release; final rebuild keeps identical binary hash |
| `python3 -u scripts/review_fix_regressions.py --binary /private/tmp/codeatlas-fix.TY5KJC/target/release/codeatlas --output /private/tmp/codeatlas-fix.TY5KJC/final-regressions.json` | 0 | 12 grouped end-to-end checks, 271.414 s total; report retained in regression artifact |
| `python3 -u scripts/review_fix_regressions.py --binary /private/tmp/codeatlas-fix.TY5KJC/target/release/codeatlas --only semantic --only wire --output docs/reports/artifacts/review-fixes-boundaries.json` | 0 | additional nested-alias and >1 MiB ingress tests; 2.528 s |
| `/opt/homebrew/bin/python3 scripts/package_macos.py --skip-build --output dist/review-fixes-final` | 0 | verified ARM64 Mach-O and system-only dynamic dependencies; honors isolated `CARGO_TARGET_DIR` |
| `/opt/homebrew/bin/python3 scripts/package_macos.py --skip-build --output /private/tmp/codeatlas-fix.TY5KJC/deterministic-final` | 0 | identical archive digest |
| `python3 scripts/phase15_package_smoke.py dist/review-fixes-final/codeatlas-0.1.0-aarch64-apple-darwin.tar.gz --output docs/reports/artifacts/review-fixes-package-smoke.json` | 0 | extracted Unicode/space path, empty PATH, seven languages, dual-era/follower/corrupt-startup/EOF/config |
| `python3 scripts/phase14_measure.py --binary /private/tmp/codeatlas-fix.TY5KJC/target/release/codeatlas --output docs/reports/artifacts/review-fixes-performance.json --file-counts 1000 --startup-samples 30 --lookup-samples 30 --watch-edit-samples 1` | 0 | generated-source sanity measurements; approved read-only child RSS sampling |
| `python3 scripts/validate_kit.py` | 0 | static prompt/skill/config/fixture/document checks, not Rust execution evidence |
| `git diff --check` | 0 | no whitespace errors |

## Current native artifact

- Binary SHA-256: `5148b860de01c893ec7a1afcfee515c0ade475f1a60d8d1045ea90f3316a77fe`.
- Binary size: 25,295,168 bytes; stripped release profile.
- Local archive: `dist/review-fixes-final/codeatlas-0.1.0-aarch64-apple-darwin.tar.gz`.
- Archive SHA-256: `56e0b3d1d31a709c10f60384e27f9c5306ceeff9a90299315b36a219c9482103`.
- Archive size: 5,570,958 bytes; checksum sidecar and checksums for all inner files.
- System dependencies only: CoreFoundation, CoreServices, libiconv, libSystem.
- Unsigned, not notarized, not publicly published. Old `dist/` artifacts were not
  overwritten; they do not contain these repairs.

Raw evidence: [regressions](artifacts/review-fixes-regressions.json),
[additional boundaries](artifacts/review-fixes-boundaries.json),
[package smoke](artifacts/review-fixes-package-smoke.json),
[performance](artifacts/review-fixes-performance.json). Every binary-based artifact
identifies the repaired binary hash, or the verified archive digest. The package
smoke's `schema_version: 1` is its report format, not SQLite's current schema v7.

## Measured sanity baseline and limitations

Host: Apple M2 Pro (10 cores), 16 GB, macOS 27.0. OS caches, power mode and background
load were not controlled; correctness/build checks were also running. Thirty
process-cold starts per MCP era give modern/legacy discovery p95 33.251/45.126 ms.
The generated 1,000-file/39,890-byte Rust corpus indexes in 4.105 s (243.602 files/s),
with sampled peak RSS 32,587,776 bytes. No-op update is 295.142 ms; one-file update
310.189 ms. Thirty warm exact lookups give p95 19.014 ms. Status during an active job
p95 is 4.831 ms (20 samples); cancellation reaches terminal `cancelled` in 31.432 ms;
shutdown after work is 10.208 ms. RSS sampling can miss brief maxima.

These are measurements, not cross-platform estimates or fresh qualification of the
old 10k/100k lookup/throughput or 10k-watcher targets. The 11k pagination corpus is a
completeness oracle, not a throughput target; 271.414 s is the whole adversarial
suite's elapsed time, not a standalone pagination latency. Syntax bindings still
do not prove runtime dispatch, macro expansion, compiler type resolution or native
grammar memory safety.

Fresh fuzz, advisory/license-policy scans, installed-Codex parsing and model-backed
Codex sessions were not run. `cargo-fuzz`, `cargo-audit` and `cargo-deny` are not
installed in the isolated toolchain: those fresh checks are unavailable, not passes.
Locked metadata notices were generated, but old Phase 14 audit/fuzz and Phase 15
installed-client results remain historical. No source was transmitted to a model
for a live Codex session. Linux/Windows native execution is still unperformed.

## Upgrade, retention and handoff

Schema v7 is transactional. Legacy IDs and dependent facts/notes survive; incomplete
v6 analysis identities occupy a separate namespace with an empty config fingerprint
so the next explicit index reparses them. The v2 extractors/resolver also require an
explicit index to obtain repaired analysis. Do not delete the database or notes to
upgrade. Obtain a consistent pre-upgrade backup before a downgrade; the old binary
does not support schema v7.

Retention bounds generation-dependent file/graph history, not database byte size:
active plus immediate predecessor and a building candidate. Notes are independent;
durable job records are not garbage-collected by this policy. An indefinitely held
WAL read transaction can retain WAL pages. Public cursors require the active
generation and become stale on activation; they do not lease old generations.
See [ADR-0014](../adr/0014-review-repair-identity-retention-and-wire-bounds.md).

Complete emitted MCP frames are ≤65,536 bytes including newline. IDs must serialize
to ≤128 bytes; oversized IDs/ingress close before echo. Prompt primary text and scope
are ≤8192/4096 UTF-8 bytes and must also fit after JSON escaping. SDK-generated
oversized responses close the transport instead of violating the cap. This input
policy is an intentional compatibility constraint, documented in MCP_CONTRACT.

No automatic next phase. The exact deferred platform prompt remains
`prompts/14-linux-windows-native-validation.md`, only when the user resumes that work.
