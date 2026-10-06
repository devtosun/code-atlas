# Test matrix — implementation evidence through Phase 15

## Review repairs — 2026-10-06

Current-source native macOS ARM64 gates: fmt, locked/offline all-target check,
Clippy with warnings denied, workspace tests (130 passed, two child-process helpers
ignored in the top-level harness but executed by parent tests), and all seven
existing xtask grammar/fixture/corpus gates pass. This is repair validation, not a
new phase. Exact commands, source provenance and limitations are in
`docs/reports/review-fixes.md`; older phase sections below are historical evidence.

| Boundary | Fresh regression evidence |
|---|---|
| Scope / binding | LF-less equal ranges terminate; cancellation guard; member versus bare-name negatives in five languages; Go directory isolation; Rust local import and nested alias shadowing |
| Extraction | Dart import/export show/hide intersection/subtraction; arrow/function-expression exports and local/default aliases in all four ECMAScript dialects; reviewed golden changes |
| Storage | schema v6→v7 preserves IDs, facts and notes; forced migration rolls back; grammar/query/extractor/config changes create immutable versions; cache hits never append facts |
| Indexing | invalid UTF-8, binary and oversized source preserve healthy active generation in full/targeted paths; real deletion succeeds; result high-water ≤ queue + batch; worker factory/panic/storage failures do not activate |
| Retention | 100 unit-test activations and 30 CLI changed/no-change generations retain active + predecessor, preserve notes/FKs and pinned WAL reads; public cursors remain generation-bound |
| Search | SQL keysets across five ranking tiers plus qualified tier; deduplicated same-name FTS documents; 11,000 distinct results across 275 pages, honest terminal state |
| Protocol / config | both MCP eras enforce prompt, escaped ID and complete frame limits; >1 MiB ingress closes without echo; malformed TOML canary never appears in diagnostics and all four config modes preserve bytes |
| Package | two deterministic native archives have the same digest; extracted package passes seven-language index, dual-era discovery, writer/follower, corrupt-startup, EOF and disposable config round-trip |

Fresh binary SHA-256:
`5148b860de01c893ec7a1afcfee515c0ade475f1a60d8d1045ea90f3316a77fe`.
Fresh archive SHA-256:
`56e0b3d1d31a709c10f60384e27f9c5306ceeff9a90299315b36a219c9482103`.
The 1,000-file sanity run measures modern/legacy startup p95 33.251/45.126 ms,
warm exact lookup p95 19.014 ms, full index 4.105 s and sampled peak RSS 32,587,776
bytes. This does not requalify the old 10k/100k or 10k-watcher performance gates.
Fresh fuzz, cargo-audit/cargo-deny, installed-Codex parsing, model-backed sessions,
Linux and Windows were not run; none are counted as fresh passes. See
`docs/reports/artifacts/review-fixes-*.json` for raw generated-corpus evidence.

## Layers
| Gate | What must be established | Initial state |
|---|---|---|
| Compatibility | all grammars load/parse/query with one runtime; modern/legacy MCP; patched SQLite/FTS5 | PASS on macOS arm64 — Phase 00 report |
| Core | path/range/ID invariants; deterministic normalization; bounded limits | PHASE 02 PASS on macOS arm64 — canonical roots, normalized relative paths, Windows-form rejection, prefix containment, source bounds and prior typed invariants |
| Storage | migrations, FTS sync, generation isolation, notes preservation, owner/follower | REVIEW REPAIR PASS on macOS arm64 — SQLite 3.53.2/FTS5/WAL, schema v1→v7, complete immutable analysis identity, rollback, bounded generation retention and preserved prior graph/lock gates; historical Phase 14 100k evidence is not rerun |
| Languages | seven languages + JSX/TSX positive/negative/partial fixtures | PHASE 07 LANGUAGE PASS on macOS arm64 — all nine providers parse and extract; 24 parser fixtures plus 8 Rust/Go, 11 JS/TS, 11 C#/Java and 8 Dart focused cases have reviewed category hashes |
| Indexing | full vs incremental equivalence, delete/rename/edit, failed traversal | PHASE 12 PASS on macOS arm64 — prior indexing gates plus watched all-language atomic save, rename/delete/recreate, case/Unicode path, ignore, burst and Git HEAD sequences converging with a clean full rebuild |
| Resolution | shadowing, overload candidates, aliases, cycles, external/dynamic cases | PHASE 09 PASS on macOS arm64 — 35 independently labelled sites, lexical/import rules, aliases/re-exports, external nodes, candidate/unresolved preservation and incremental/full equivalence |
| Retrieval | exact/prefix/FTS ranking, graph cycles, response and cursor budgets | PHASE 10 PASS on macOS arm64 — ranked literal-safe search, repository/generation/query-bound cursors, symbol/outline/reference/call/map/impact reads, fresh-source verification, overlap-deduplicated context and exact final serialization caps |
| Protocol | both lifecycle eras, tools/resources/prompts, cancellation, EOF | PHASE 13 PASS WITH LIMITATION — modern/legacy full-binary tests cover all sixteen opted-in tools, default-hidden writes, four resource forms, three prompts, version-correct results and prior EOF/cap gates; actual Codex UI exposure was not run |
| Security | escapes, secret excludes, injection-as-data, invalid requests, resource caps | PHASE 14 PASS on supported macOS ARM64 — generated boundaries, dual-era adversarial tools/resources and three bounded fuzz targets pass; native-code/TOCTOU residuals remain documented |
| Reliability | locked/corrupt/read-only DB, disk errors, killed writer, watcher overflow | PHASE 14 PASS on supported macOS ARM64 — killed writer preserves active data and notes, migration rollback, generation isolation, cancellation and watcher recovery pass; physical power loss remains untested |
| Release | clean-machine native launch, packaged grammars, preserved Codex config | PHASE 15 macOS ARM64 PASS — deterministic archive and extracted-package dual-era/index/follower/corrupt-startup/EOF/config tests pass; installed Codex parses a disposable entry; model-backed session not run; Linux/Windows deferred |

## Precision measurement
Keep a hand-labelled fixture corpus with declarations, reference sites, candidate
sets and known dynamic/unresolvable cases. Measure declaration precision/recall,
reference binding precision/recall and unresolved rate separately for each language.
Report numerator and denominator. Excluding dynamic cases from resolved precision is
fine only when the excluded cases and unresolved rate remain visible. A unit-test
pass rate is not a real-world semantic accuracy score. Small fixtures are not proof
of production-level language coverage.

Use independent labels/negative examples, not the current parser output as its own
ground truth. Required fixture assertions must pass exactly; real-repository scores
are measurements with corpus provenance and limitations, not invented release claims.

## Proposed performance gates (targets, not measurements)
Hardware baseline must be recorded: CPU, RAM, OS, filesystem, power mode, release
binary hash, cache warmth, source size/files, grammar set and background load.
- Transport availability: p95 <= 1 second over 30 cold process starts, empty or busy DB;
  3-second integration deadline as a safety gate on the recorded baseline.
- Status and tools/list remain responsive while a large indexing job runs.
- Warm exact symbol lookup: target p95 <= 100 ms on 100k indexed declarations.
- Graph request: obey depth/node/time/output limits; no unbounded scan.
- Single-file edit: target p95 <= 500 ms on a documented 10k-file fixture after debounce,
  with parse/re-resolution/storage times reported separately. If full re-resolution
  exceeds it, report the failure and optimize later; do not weaken correctness.
- Idle/no-watch: no busy loop. Measure CPU and resident memory; no invented percentages.
- Response cap: <= 65,536 serialized bytes in default policy, including wrapper/fallback.
- Index throughput and peak RSS: report actual values on 10k/100k-file generated corpus;
  no absolute throughput requirement until phase 14 establishes a baseline.

Flaky wall-clock microbenchmarks must not masquerade as correctness tests. Keep
hard resource bounds in deterministic tests; benchmark regression thresholds in a
controlled runner. Check cancellation latency and verify work stopped, not just
that the client received a fast response.

## Cross-platform matrix
Current supported target: macOS ARM64. Deferred qualification targets: Windows x64
MSVC and Linux x64. Run path, lock, watcher, FTS,
stdio subprocess, shutdown and release smoke tests natively. Cross-compiling is
not execution evidence. Network-mounted sources are degraded/limited and the DB
must remain local. Native watcher guarantees never replace reconciliation tests.

Phase 00 native compatibility status:

| Platform | Grammar / MCP / SQLite spike | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/00-compatibility-spike.md` |
| Linux x64 | NOT RUN | native runner unavailable in Phase 00 |
| Windows x64 MSVC | NOT RUN | native runner unavailable in Phase 00 |

Phase 01 native workspace/lifecycle status:

| Platform | Build / CLI / stdio lifecycle | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/01-workspace-and-lifecycle.md` |
| Linux x64 | NOT RUN | `ubuntu-24.04` CI job configured; workflow not executed from this non-Git directory |
| Windows x64 MSVC | NOT RUN | `windows-2025` CI job configured; workflow not executed from this non-Git directory |

Phase 02 native repository/storage status:

| Platform | Paths / lock / SQLite generations | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/02-safe-repositories-and-storage.md`; 17 focused repository/storage tests plus a spawned follower-process helper and workspace gates |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native junction, ACL and file-lock behavior requires the configured Windows runner |

Phase 02 also reran the status-only subprocess lifecycle after production SQLite and
gix were linked. All 30 discovery starts remained below the existing three-second
safety deadline; this rerun is a regression gate, not a new release benchmark.

Phase 03 native parser-kernel status:

| Platform | Registry / parser bounds / fixture golden | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/03-parser-kernel.md`; 10 parser-kernel tests, 3 xtask tests and all 24 fixture entries |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native grammar compilation/execution requires the configured Windows runner |

Phase 03 checks valid UTF-8, BOM/Unicode/CRLF byte positions, empty and malformed
input, ERROR/MISSING diagnostics, deterministic owned output, source/capture/tree/
native-progress bounds, cooperative cancellation and parser reset. These are
deterministic safety checks, not wall-clock timeout guarantees.

Phase 04 native Rust/Go extraction status:

| Platform | Rust/Go declarations / scopes / imports / references / calls | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/04-rust-and-go.md`; 13 language tests, 8 focused extraction fixtures, 24 parser regressions |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native grammar compilation/execution requires the configured Windows runner |

Phase 04 checks stable declaration IDs under line insertion, distinct shadowed
bindings and same-name receiver methods, exact aliases/containers/signatures,
unresolved call targets, cfg/build-tag limitations, comments/strings, incomplete
edits and UTF-8 identifier ranges. The corpus is a deterministic contract, not a
real-world semantic precision measurement.

Phase 05 native JavaScript/TypeScript extraction status:

| Platform | JS/JSX/TS/TSX declarations / scopes / modules / references / calls | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/05-javascript-and-typescript.md`; 16 language tests, 11 focused extraction fixtures, 24 parser regressions and 8 Rust/Go regressions |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native grammar compilation/execution requires the configured Windows runner |

Phase 05 checks explicit extension/dialect routing and distinct TS/TSX grammars,
same-name type/value roles, ESM aliases/re-exports, reviewed CommonJS, overloads,
decorators, TSX generics, JSX tag non-call limitations, unresolved optional/computed/
dynamic calls, comments/strings, malformed edits, Unicode ranges and cooperative
cancellation in all four affected dialects. No JavaScript configuration or package
tool is executed. The corpus remains a deterministic contract rather than a
real-world precision/recall score.

Phase 06 native C#/Java extraction status:

| Platform | C#/Java declarations / scopes / imports / references / calls | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/06-csharp-and-java.md`; 20 language tests, 11 focused C#/Java extraction fixtures, 24 parser regressions, 8 Rust/Go and 11 JS/TS regressions |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native grammar compilation/execution requires the configured Windows runner |

Phase 06 checks distinct C# partial IDs with evidence-only grouping hints, overload
signatures and nested containers, C# extension/local/async/top-level/preprocessor
syntax, Java package/static imports, annotations, lambdas and non-call method
references, unresolved interface/virtual/extension dispatch, malformed recovery,
Unicode CRLF ranges and cooperative cancellation. No .NET/JVM/compiler/build/
restore/project tooling is executed. The corpus remains a deterministic contract,
not a semantic precision/recall measurement.

Phase 07 native Dart extraction status:

| Platform | Dart declarations / scopes / library relationships / references / calls | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/07-dart.md`; 24 language tests, 8 focused Dart extraction fixtures, 24 parser regressions and every earlier focused language suite |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native grammar compilation/execution requires the configured Windows runner |

Phase 07 checks libraries/imports/exports/aliases, part/part-of syntax, distinct
ordinary/named/factory/redirecting constructors and getter/setter identities,
generics, async/await, records, patterns, sealed classes, extension types, lexical
shadowing, prefix evidence, nested Flutter-style calls/named arguments/callbacks,
malformed recovery, multibyte UTF-8 plus CRLF byte positions and cooperative
cancellation/reset. Capitalized calls, package/part relations and Flutter widget
nesting remain unresolved. The pinned grammar's non-ASCII identifier failure is an
explicit limitation. No Dart/Flutter SDK, analyzer, pub, build_runner or generated
code is used. The corpus is a deterministic contract, not a semantic accuracy score.

Phase 08 native persistent-index status:

| Platform | Scan / parse / persistent generation / durable jobs | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/08-index-jobs.md`; schema v3, nine language/dialect seeds, incremental mutations, cancellation, readers/locks and killed-writer recovery |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native junction, ACL, file-lock and process behavior requires the configured Windows runner |

Phase 08 checks exact content/config/grammar/query/extractor reuse rather than
mtime/size, complete traversal before deletion inference, bounded source queue and
worker counts, batch persistence, syntax-only coverage, active-generation rollback,
FTS/fact membership, durable progress/request keys and cooperative cancellation in
an executing worker. The tests establish deterministic correctness on small fixtures;
they are not throughput or peak-RSS measurements.

Phase 09 native resolution/graph status:

| Platform | Lexical/import resolution / persistent graph | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/09-resolution-and-graph.md`; schema v4, 35 labelled sites, candidates/cycles/bounds and changed-file/full equivalence |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native filesystem, SQLite lock and grammar execution require the configured Windows runner |

The labelled corpus has five sites per required language. Syntax coverage is 5/5
for each language. Binding precision and recall on supported labelled targets are:
C# 6/6, Dart 3/3, Go 4/4, Java 5/5, JavaScript 3/3, Rust 4/4 and TypeScript 4/4.
Unresolved rates are Dart 1/5, JavaScript 2/5 and TypeScript 1/5, and 0/5 for C#,
Go, Java and Rust. Candidate and external-observation counts remain separately
reported in `docs/reports/artifacts/09-resolution-evaluation.json`. These exact
fixture results demonstrate the implemented rules; they are not real-world or
compiler-level semantic accuracy estimates.

Phase 10 native retrieval/context status:

| Platform | Ranked retrieval / fresh source / context budget | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/10-retrieval-and-context.md`; literal-safe FTS, deterministic cursors, rich graph reads, freshness errors, overlap deduplication and exact wire-projection byte caps |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native filesystem, SQLite and source-read behavior require the configured Windows runner |

The Phase 10 adversarial corpus covers empty/oversized/punctuation/Unicode search,
same-name ordering, malformed/cross-query/stale/foreign cursors, candidate opt-in,
cycles/fanout, changed/deleted/excluded source, overlapping snippets and escaped JSON
that expands in the text fallback. These are deterministic correctness checks on
small fixtures, not 100k-symbol latency or memory measurements.

Phase 11 native MCP tool status:

| Platform | Typed tools / detached jobs / dual-era wire contracts | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/11-mcp-tools.md`; all thirteen tools through modern and legacy subprocess clients, generated schemas, application/protocol error separation, idempotency, writer contention, cancellation and EOF |
| Linux x64 | NOT RUN | only `aarch64-apple-darwin` is installed locally; configured CI is not dispatchable from this non-Git directory |
| Windows x64 MSVC | NOT RUN | native process, lock, path and SQLite behavior require the configured Windows runner |

The Phase 11 wire corpus indexes Rust and TypeScript fixture source, then exercises
status, job creation/status/cancel, search, symbol, references, calls, outline,
fresh-source read, repository map, impact and context in both protocol eras.
Malformed fields, unsupported versions, invalid enums/ranges, foreign job IDs,
no-index reads and locked writers produce protocol or stable application errors as
appropriate. Full stdout lines are parsed as JSON, tool responses remain at or below
65,536 bytes, and EOF during an active 300-file job exits after cooperative cancel
and join. The installed `codex-cli 0.154.0` was inspected but no actual model/client
session was run.

Phase 12 native watcher status:

| Platform | Native/poll watcher / reconciliation / owner lifecycle | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS WITH LIMITATION | `docs/reports/12-watch-and-incremental.md`; FSEvents backend initialization, PollWatcher events, bounded overflow/lost-event recovery, all-language full equivalence, owner takeover/crash and EOF cleanup |
| Linux x64 | NOT RUN | inotify behavior requires the configured native Linux runner |
| Windows x64 MSVC | NOT RUN | ReadDirectoryChangesW, case/junction and process-lock behavior require the configured native Windows runner |

The deterministic watch corpus covers atomic replacement, rename, delete/recreate,
case-only rename through an intermediate path, a Unicode filename, `.codeatlasignore`
change, 40-write burst and simulated Git HEAD transition. The explicit polling run
used a 50 ms debounce and 1,000 ms reconciliation interval and converged in
1,137.715625 ms on the recorded debug macOS arm64 host; active file/fact/symbol counts
matched an independent clean full rebuild. This is one small-fixture observation, not
the proposed 10k-file p95 target. The native FSEvents backend initialized but did not
deliver a callback during the managed test's short burst, so native event latency is
not claimed; periodic reconciliation still ran and the limitation remains visible.
See `docs/reports/artifacts/12-watch-latency.json`.

Phase 13 native memory/resource/prompt status:

| Platform | Revisioned memory / resources / prompts | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/13-memory-resources-prompts.md`; schema-v5 migration, exact revisions/evidence, stale filtering, retention, URI attacks, static prompts and all sixteen opted-in tools in both protocol eras |
| Linux x64 | NOT RUN | native SQLite/process execution requires the configured Linux runner |
| Windows x64 MSVC | NOT RUN | native ACL/path/process behavior requires the configured Windows runner |

The storage corpus creates a verified symbol-backed note, rejects missing/conflicting
revisions, changes the active generation to make the evidence stale, proves default
stale exclusion, preserves the note through GC and deletes it only at the exact
revision. Existing migration/backup tests cover note preservation. Modern and legacy
subprocess clients list the default 14-tool surface and opted-in 16-tool surface,
exercise valid/invalid/oversized writes, restart retention, four resource forms,
traversal/encoded-scheme rejection and all three prompt templates. A synthetic note
and prompt argument asks for secret upload; it is preserved only as explicitly
labelled untrusted data. This does not prove remote-model prompt-injection immunity.

Phase 01 measured 30 sequential debug-binary starts from spawn to modern discovery
response against empty temporary roots: p50 4.393291 ms, p95 4.875125 ms, maximum
270.550041 ms. Every sample met the three-second safety deadline. This is a local
baseline, not an optimality or cross-platform claim; see
`docs/reports/artifacts/01-startup-baseline.json`.

Phase 14 native hardening/evaluation status:

| Platform | Adversarial / recovery / release measurement | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS | `docs/reports/14-hardening-and-evaluation.md`; workspace, dual-era boundary, recovery, fuzz, supply-chain, accuracy and repaired 10k/100k native gates executed |
| Linux x64 | NOT RUN / UNSUPPORTED | deferred to `prompts/14-linux-windows-native-validation.md` |
| Windows x64 MSVC | NOT RUN / UNSUPPORTED | deferred; junction/ACL/process-lock behavior remains unexecuted |

The copied release binary SHA-256 is
`ec90f1123317a56c3a054745c0f27b08b4240cb7c632742ceb02249640ff751e`.
Modern startup p95 is 13.700 ms (393.564 ms maximum); legacy startup p95 is
6.910 ms (6.964 ms maximum), both over 30 process-cold samples. The 10k corpus
indexed at 1,000.631 files/s with sampled 97,320,960-byte peak RSS; the 100k corpus
indexed at 907.137 files/s with sampled 535,855,104-byte peak RSS. Status p95 during
an active job is 0.143 ms, cancellation reaches terminal `cancelled` in 385.353 ms,
and shutdown after work is 19.597 ms.

The repaired 100k warm exact-symbol lookup p95 is 60.323 ms, passing the 100 ms
target. Ten 10k-corpus watcher edits have a post-debounce reconciliation p95 of
214 ms, passing the 500 ms target; their event-to-activation p95 is separately
2,326.930 ms because the controlled runner forces the two-second polling fallback.
Each job parses one file and atomically retains all 10k memberships. Startup, status,
response-size, cancellation and scale-correctness checks pass.
See `docs/reports/artifacts/14-performance-baseline.json` for raw samples, host,
cache policy and RSS method.

The fixture gate observes 157/157 required declaration anchors and zero hits across
23 forbidden observation checks. The hand-reviewed parser golden exhaustively labels
every emitted declaration kind, spelling and byte range across all nine dialects;
the exact snapshot gate supplies declaration-precision numerators/denominators.
Existing independently labelled reference-binding metrics cover 35 sites across
seven required languages. A source-hash-locked five-file, 6,083-byte corpus from this
authorized project adds exhaustive Rust labels. The 100k generated corpus remains
capacity evidence, not real-world semantic accuracy. See the accuracy artifact.

The macOS-target locked metadata inventory contains 241 third-party registry
packages, no missing declared license metadata and no Git/path third-party source.
Cargo-audit scans 294 locked dependencies against 1,251 advisories with no findings;
cargo-deny passes advisories, bans, licenses and sources. Three isolated five-minute
fuzz targets build and complete with zero crashes/timeouts. These bounded runs are
not a proof of native grammar memory safety. See the dependency and fuzz artifacts.

Phase 15 native package/integration status:

| Platform | Native package / extracted MCP / Codex config | Evidence |
|---|---|---|
| macOS 27.0 arm64 | PASS WITH CLIENT LIMITATION | `docs/reports/15-release-and-codex.md`; deterministic archive, 7-language index, both protocol eras, follower/corrupt-startup/EOF and safe config round-trip pass |
| Linux x64 | NOT RUN / UNSUPPORTED | deferred to `prompts/14-linux-windows-native-validation.md`; no package produced |
| Windows x64 MSVC | NOT RUN / UNSUPPORTED | deferred; native replacement, ACL, junction and package behavior unexecuted |

The extracted unsigned archive runs from a Unicode/space path under an empty `PATH`;
no Rust, Dart, Node, Go, Java or .NET SDK is visible. It indexes seven generated
language fixtures with zero failures and serves 14 default tools over modern
2026-07-28 and legacy 2025-11-25 MCP. A concurrent follower remains queryable, corrupt
storage does not block discovery/tools-list, and EOF exits cleanly. `toml_edit` unit
and package tests preserve unrelated comments/settings/MCP entries, reject malformed
TOML and unowned name collisions, set `required=false`, back up apply, and remove only
the marked entry while source/notes sentinels survive.

The native binary SHA-256 is
`45f11cecbadafc64456fd06ea36dcca9e140737d808f42ac56878afee6227b82`;
the 5,484,074-byte archive SHA-256 is
`57e64086ba5fb066f0fd79f1a2aeda71080e7cb68e8a7c8a7ca023327485c427`.
Repackaging the same binary produces the same archive digest. Installed
`codex-cli 0.154.0` parses the entry in a disposable `CODEX_HOME`; no model-backed
session ran because fixture-source transmission was not explicitly authorized.

## Evidence reports
Each report includes git revision (or explicitly dirty state), toolchain, target,
commands, exit codes, elapsed time, fixture/corpus identifiers, failures, limitations,
and links to logs. Do not store private production source in reports or CI artifacts.
