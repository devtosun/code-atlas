# Project state

Review repairs (2026-10-06): CR-001–CR-013 from
`docs/reviews/CODE_REVIEW_REPORT.md` are implemented and regression-tested against
freshly compiled, locked native macOS ARM64 source. Workspace gates pass (130 tests,
plus two subprocess helpers exercised by their parent tests), all seven existing
xtask language/corpus gates pass, and the updated extracted package passes both MCP
eras. Schema v7 preserves legacy version IDs/notes while invalidating incomplete
analysis cache keys. Reindex explicitly after upgrading; back up before any
downgrade. Search now traverses all 11,000 oracle results beyond the former ceiling.
See `docs/reports/review-fixes.md` and ADR-0014 for fixes, exact hashes, retention,
wire limits and fresh evidence. Historical Phase 14/15 hashes and measurements below
remain historical, not validation of this repaired binary. No real Codex config,
commit, push, publication or subsequent phase was performed in this repair turn.

Implementation status: PHASE 15 COMPLETE FOR THE SUPPORTED macOS ARM64 TARGET. The
locked native workflow now produces a deterministic unsigned archive with the
stripped executable, quick-start, privacy guide, dependency notices, build metadata
and checksums. The extracted archive passes seven-language indexing, modern/legacy
stdio, owner/follower, corrupt-storage startup, EOF and safe Codex config round-trip
tests from Unicode/space paths with no language SDK visible. The installed
`codex-cli 0.154.0` parses a disposable config entry. No real user config changed and
no model-backed session, signing, notarization or publication was performed. Linux
x64 and Windows x64 MSVC remain explicitly deferred and unsupported.

Current requested phase: none.
Current requested work: review repairs complete; no optional phase selected.
Last completed implementation phase: 15 — native packaging and safe Codex integration on macOS ARM64.
Last attempted phase: 15 — completed with the model-backed Codex smoke explicitly not run.
Next prompt: `prompts/14-linux-windows-native-validation.md` when those targets are resumed.
Deferred native qualification: `prompts/14-linux-windows-native-validation.md`.

## Phase ledger
| Phase | State | Evidence |
|---|---|---|
| 00 | completed | `docs/reports/00-compatibility-spike.md`; locked spike and raw frames |
| 01 | completed | `docs/reports/01-workspace-and-lifecycle.md`; locked workspace and subprocess tests |
| 02 | completed | `docs/reports/02-safe-repositories-and-storage.md`; native path/lock and real SQLite tests |
| 03 | completed | `docs/reports/03-parser-kernel.md`; production registry, bounded parser tests and reviewed fixture golden |
| 04 | completed | `docs/reports/04-rust-and-go.md`; scoped source extraction and reviewed Rust/Go golden |
| 05 | completed | `docs/reports/05-javascript-and-typescript.md`; role-aware JS/TS extraction and reviewed golden |
| 06 | completed | `docs/reports/06-csharp-and-java.md`; uncertainty-preserving C#/Java extraction and reviewed golden |
| 07 | completed | `docs/reports/07-dart.md`; syntax-only Dart/Flutter-style extraction and reviewed golden |
| 08 | completed | `docs/reports/08-index-jobs.md`; persistent bounded indexing and durable-job evidence |
| 09 | completed | `docs/reports/09-resolution-and-graph.md`; bounded resolver, schema-v4 graph and labelled evaluation |
| 10 | completed | `docs/reports/10-retrieval-and-context.md`; ranked search, bound cursors, fresh reads and exact response caps |
| 11 | completed | `docs/reports/11-mcp-tools.md`; thirteen real tools, detached jobs and dual-era subprocess contracts |
| 12 | completed | `docs/reports/12-watch-and-incremental.md`; bounded opt-in watching, reconciliation and owner recovery |
| 13 | completed | `docs/reports/13-memory-resources-prompts.md`; revisioned notes, typed resources/prompts and dual-era wire tests |
| 14 | completed | `docs/reports/14-hardening-and-evaluation.md`; supported macOS ARM64 gates pass; Linux/Windows deferred and unsupported |
| 15 | completed | `docs/reports/15-release-and-codex.md`; macOS ARM64 package/integration gates pass, model-backed client session not run |
| 16–17 optional | not selected | requires explicit phase request |

## Decisions already selected
Rust; official rmcp; Tree-sitter for seven languages; stdio without mandatory daemon;
SQLite/FTS5/WAL; compile-time language adapters; explicit root authorization; bounded
queries and atomic generations; optional memory writes; cross-platform native tests.

## Phase 00 decisions

Rust 1.98.1; Tree-sitter 0.27.0; published grammar bindings listed in
`docs/GRAMMAR_MATRIX.md`; official rmcp 3.4.0 with `server` and `transport-io`;
rusqlite 0.40.2 with `bundled-full` and SQLite 3.53.2. The published Dart binding
comes from `nielsenko/tree-sitter-dart`, not the similarly named alternative.
Exact checksums, commits, query hashes and compile options are recorded in
`config/dependency-lock.json`.

## Phase 01 decisions

Rust 1.98.1 is pinned for a seven-member workspace. `ca-core` contains validated
IDs, byte ranges, bounded limits, cooperative cancellation and process lifecycle
types without rmcp, SQLite or Tree-sitter dependencies. The CLI composition root
uses exact dependency pins and starts official rmcp before any repository work.
Only `serve --root` and `doctor --json` are implemented. The server advertises only
`repository_status`; tracing is stderr-only. Native CI is configured for
`ubuntu-24.04`, `macos-15` and `windows-2025`, but only macOS arm64 has run locally.

## Phase 02 decisions

Repository roots are canonicalized once and source paths use bounded normalized
components. The reader and scanner do not follow symlinks/reparse points, revalidate
opened files, enforce source-extension/secret/generated exclusions, and honor
`.gitignore`, `.ignore` and `.codeatlasignore`. `gix` is isolated and narrowly
featured for worktree metadata; Gitless roots remain valid and linked worktrees get
distinct BLAKE3 identities.

Application data resolves outside the source tree. SQLite schema version 2 stores
immutable file versions, test facts/search documents, generations, jobs and durable
memory scaffolding. The real bundled SQLite 3.53.2 is checked against the 3.51.3
safety floor and FTS5 is probed at open. One `fs4` OS lock owns a dedicated writer
thread; followers use query-only connections and receive `WRITER_BUSY` for writes.
Activation and active-generation reads are transactional, `synchronous=FULL` is the
default, and backups use the SQLite backup API. Parser facts are not wired into these
generations until the indexing phase.

## Phase 03 decisions

Tree-sitter 0.27.0 and the exact Phase 00 grammar pins are now production
dependencies isolated in `ca-languages`; core, storage and protocol crates do not
depend on Tree-sitter. The registry distinguishes JavaScript/JSX and
TypeScript/TSX, with the dedicated TSX grammar. Parser instances and compiled query
objects are worker-local. Limits cover source bytes, native progress callbacks,
syntax traversal, in-progress query matches, diagnostics and returned captures.
Cancellation and budgets are cooperative and are not described as hard timeouts;
the parser is reset whenever native parsing/query work is interrupted.

Phase 03 returns owned syntax-only observations with original spelling, source hash,
zero-based half-open UTF-8 byte ranges, one-based lines and zero-based byte columns.
Invalid UTF-8 is rejected; BOM and CRLF bytes are retained without normalization.
The reviewed `symbols.scm` assets provide only fixture declaration anchors. Scopes,
imports, references, calls and complete declaration extraction remain explicitly
pending for phases 04–07, so at Phase 03 completion no provider claimed extractor readiness.

## Phase 04 decisions

Rust and Go use four separately compiled query assets per language: symbols/scopes,
imports, references and calls. The adapter expands Rust grouped `use` trees, records
Go package/import aliases, attaches declarations/references/calls to the narrowest
captured lexical scope, and stores method containers, receiver types and source
signatures. IDs hash language, repository-relative path, category, kind, spelling
and a named-node structural path; inserting lines without changing tree structure
does not change declaration IDs. Structural sibling edits can still change IDs.

All calls remain `unresolved` with no target ID. Reference rows are syntax
observations only; matching names do not prove trait/interface dispatch. Rust macro
invocations are observed without expanding token trees. Rust cfg/cfg_attr and Go
build tags are retained as conditions with warnings that the active configuration is
unknown. The `rust-source-v1` and `go-source-v1` extractor versions are combined
with ordered query bytes into extractor fingerprints.

## Phase 05 decisions

JavaScript, JSX, TypeScript and TSX each compile separate symbols/imports/references/
calls query assets. Extension routing remains explicit, with TS and TSX retaining
distinct grammar fingerprints. One ECMAScript-family adapter normalizes declarations,
scopes, ESM imports/exports/re-exports, reviewed CommonJS, references and unresolved
calls. TypeScript attaches explicit type/value/namespace roles and preserves overload
declarations, generics and decorators without type checking.

JSX opening/self-closing tags are references with `jsx_tag_not_a_call_edge`, never
implicit call edges. Re-exports require cycle guards; computed targets and dynamic
module specifiers remain unresolved. The adapter does not execute JavaScript,
configuration, Node/npm, package scripts or module loaders, and it does not resolve
`tsconfig` paths or package exports. `javascript-source-v1` and
`typescript-source-v1` are combined with each dialect's ordered query bytes in the
extractor fingerprint.

## Phase 06 decisions

C# and Java each compile four reviewed query assets for declarations/scopes,
imports, references and calls. C# partial declarations retain distinct path-sensitive
IDs and only share a `partial_group_hint` with an explicit unconfirmed-group
limitation. Callable signatures and structural paths preserve overload and nested
declaration identity without compiler symbol keys.

Member-call receiver expressions are retained as source evidence, not inferred
types. Interface, virtual and C# extension dispatch remain unresolved with no target
ID. C# preprocessor expressions are recorded without choosing active branches;
attributes/annotations are not executed, and Java method references are observations,
not call edges. Neither adapter invokes .NET, a JVM, compiler, build/project tooling,
dependency restore, annotation processing or DI configuration. `csharp-source-v1`
and `java-source-v1` are combined with ordered query bytes in extractor fingerprints.

## Phase 07 decisions

Dart now compiles separate symbols/scopes, library relationships, references and
calls queries against the pinned `nielsenko/tree-sitter-dart` 0.2.0 grammar. It
extracts libraries, imports/exports, aliases, part/part-of links, classes, enums,
mixins, extensions and extension types, functions/methods, getters/setters,
ordinary/named/factory/redirecting constructors, parameters, pattern bindings,
generics, async/await and lexical scopes.

Constructor declarations keep structural identities, owners, signatures and kinds;
capitalized calls remain unresolved constructor-like syntax because optional `new`
does not prove a constructor or Flutter widget. Import-prefix/member receivers are
source expressions, not inferred types. Flutter-style nesting, named arguments and
callbacks are retained without producing a semantic widget tree. No Dart/Flutter
SDK, analyzer, pub, build_runner, package configuration or generated code is loaded
or executed. `dart-source-v1` is combined with the ordered query bytes in the
extractor fingerprint.

The pinned grammar parses the required records, patterns, sealed classes, extension
types and switch expressions in the reviewed corpus. It reports `ERROR` for non-ASCII
Dart identifiers, which remains an explicit grammar limitation; UTF-8 in comments
and strings and CRLF byte positions are still preserved and tested.

## Phase 08 decisions

Schema version 3 adds language/fingerprint/coverage fields to immutable file
versions; durable index-job modes, states, counters, cancellation and request keys;
owned symbols, scopes, imports, references, call sites, conditions and diagnostics;
and generation coverage/config/extractor metadata. The generic Phase 02 fact and FTS
tables remain populated from declarations for compatibility and retrieval work.

`ca-engine::indexing` owns the synchronous use case behind an `IndexStore` port.
The CLI composition root adapts SQLite and `ca-languages` without making the engine
depend on either. A bounded source queue feeds worker-local parsers, results persist
in batches, and no AST or whole-repository source corpus is accumulated. Traversal
must finish before deletion inference. Read/hash happens again before parsing, so an
identical mtime cannot cause stale reuse and an edit between scan and parse is visible.

Full and incremental jobs create candidate generations. A failed scan, cancellation,
storage error or interrupted owner abandons the candidate and retains the prior active
generation. Per-file parser failures persist explicit failed coverage and diagnostics;
the activated generation remains honestly `ready_with_warnings`. Phase 08 performs
no cross-file resolver work and emits no fabricated semantic edges.

The CLI runs indexing synchronously and reports the durable job/generation result.
This is not the future MCP `index_repository` asynchronous tool, and no watcher,
search tool, memory tool or daemon is advertised.

## Phase 09 decisions

`ca-engine::resolution` owns deterministic lexical/import rules and bounded graph
traversal behind storage ports. Every edge retains its source observation, exact
range, rule and resolver versions, evidence, resolution label and candidate count.
Local targets must be immutable symbol versions in the same candidate generation;
SQLite foreign keys and writer-side validation reject stale or cross-generation
targets. External static imports are explicit external nodes, while unsupported,
computed or dynamic targets remain unresolved with diagnostics.

The first resolver version intentionally performs full-generation re-resolution
after every incremental parse. Supported root `go.mod` module and `pubspec.yaml`
package declarations are read as inert UTF-8 data; no SDK, compiler, package manager,
build script, configuration code or repository instruction is executed. Relative
ECMAScript/Dart modules, Rust source modules and syntax-observed Java/C#/Go package
relationships use narrow rules. Package export maps, path aliases, conditional build
selection, type inference and virtual/interface/trait dispatch remain unsupported.

Candidate edges are stored and queryable but excluded from graph reads by default.
Traversal is cycle-safe with explicit depth, node and edge caps. The independent
fixture labels cover 35 sites (five for each required language), and the checked-in
metrics report syntax coverage, binding precision/recall, unresolved rates and sample
sizes separately rather than claiming compiler or production-corpus accuracy.

## Phase 10 decisions

`ca-engine::retrieval` owns generation-bound retrieval policy behind a narrow store
port. SQLite executes parameterized, bounded reads against one pinned snapshot;
exact qualified/name/case/prefix tiers precede literal-token FTS5 ranking. Identifier
tokenization splits camelCase, acronym boundaries and punctuation-separated names.
The index configuration fingerprint is now
`codeatlas-default-index-config-v2-search-tokens`, forcing refreshed derived search
documents without changing schema version 4.

Opaque cursors contain an operation, repository ID, generation ID, query/filter
fingerprint and deterministic sort tuple. Foreign, stale, malformed and cross-query
cursors fail explicitly. Graph traversal is cycle-safe and bounded by depth, nodes,
edges and a cooperative deadline. Candidate relations require opt-in; missing callers
is explicitly not treated as proof of dead code.

`read_code` revalidates the current authorized source through the existing no-follow,
ignore-aware reader and compares its BLAKE3 content hash with the pinned file version.
Changed, deleted, newly excluded and non-indexed sources are distinct failures.
`build_context` ranks symbols, deduplicates overlapping evidence ranges, retains
provenance/coverage/uncertainty and reports an explicit `ceil(UTF-8 JSON bytes / 4)`
token estimate. The authoritative limit is the exact serialized byte count of
structured content plus its JSON text fallback; results are reduced structurally,
never cut into invalid JSON or UTF-8.

No Phase 10 retrieval method is advertised over MCP yet. Phase 11 must add thin rmcp
handlers and protocol integration tests without moving these policies into transport
code.

## Phase 11 decisions

`ca-mcp` owns closed serde/schemars inputs, accurate annotations, a common
application envelope and stable application-error mapping. It delegates typed
requests through a backend port and moves every blocking operation to Tokio's
blocking executor; rmcp still owns JSON-RPC framing, lifecycle negotiation and
protocol errors. The CLI backend adapts the existing engine/storage/language ports
without adding SQL to handlers or accepting a caller-supplied root.

Indexing now has separate durable prepare and run-prepared steps. MCP returns the
created or deduplicated job before traversal and retains the owner storage handle,
cooperative token and worker handle. Same-key active requests reuse the job,
different writers receive retryable `WRITER_BUSY`, and EOF cancels and joins every
worker. These are explicitly application jobs, not MCP Tasks.

Modern 2026-07-28 and legacy 2025-11-25 subprocess clients run all thirteen tools
against persisted fixture data. Tests also cover schema/annotation shape, invalid
fields/enums/ranges, no-index and foreign-job errors, contention, cancel/commit
races, stdout purity and complete emitted frame sizes. The installed
`codex-cli 0.154.0` and its MCP help were inspected, but an actual Codex/model MCP
session was not run and no user configuration was changed.

## Phase 12 decisions

Watching is a trusted startup opt-in (`serve --watch`); `auto_index` and `watch`
remain false by default and no tool can broaden the authorized root. The watcher is
created only after this process acquires and retains the database write-owner lock.
Followers remain query-only and never start a competing watcher; ownership is
released on clean EOF or process death and can be acquired by a later process.

notify 8.2.0 supplies the native backend and content-comparing PollWatcher fallback.
Callbacks feed a bounded synchronous queue; saturation is counted rather than hidden.
Hints are debounced and coalesced, while periodic reconciliation always reruns the
ignore-aware scan and hashes source bytes. Atomic saves, rename/delete/recreate,
case-only renames, Unicode paths, ignore changes, burst writes and Git HEAD changes
therefore converge through the same incremental indexing path. Changed files are
fully reparsed and every candidate generation receives full graph re-resolution;
Tree-sitter old-tree reuse is not attempted.

`repository_status` reports backend, running/pending/reconciling state, queue and
overflow counters, reconciliation counts, last error/trigger, and the explicit
degraded event-delivery guarantee. A retained owner storage handle and one
serialization gate prevent overlapping generation writes. EOF cancels active jobs,
drops/joins the watcher coordinator and workers, then releases ownership.

## Phase 13 decisions

Schema version 5 expands the persistent memory tables with
decision/convention/pitfall/task kind, revision, author, origin and scope fields.
Evidence keeps an exact repository-relative path and content hash plus an optional
immutable symbol observation ID. Evidence is accepted only when it matches the
active generation; later path/hash/symbol changes yield `stale`, while notes without
evidence or without a usable generation are `unverified`. No name-based symbol
rebinding occurs. Index activation, abandoned-generation GC, migration, backup and
restart do not own or delete memory rows.

`search_memories` is always available and excludes stale notes by default. Trusted
startup flag `serve --memory-write` exposes and independently authorizes
`upsert_memory` and destructive `forget_memory`; the default tool list contains 14
tools and the opted-in list contains all 16. Writes enforce 16 KiB text, 10,000
record, 32 evidence and metadata limits plus optimistic revisions. Notes are always
labelled explicitly authored untrusted data and never become extracted code facts.

Resources expose exact status/map URIs and typed symbol/memory URI templates. A
strict parser rejects other schemes, encoding, traversal and path-like IDs; reads
delegate to the same repository-bound backend use cases and response budget as
tools. `explain_symbol`, `plan_change` and `investigate_failure` are static client
prompt templates that preserve supplied text as quoted untrusted data, request
evidence/uncertainty and defer every mutation/approval decision to the client. The
server does not call a model and no client UI exposure is assumed.

## Phase 14 decisions

Schema version 6 adds indexes for orphan file-version cleanup and folded symbol-name
lookup. The cleanup index removes a measured quadratic 100k-generation writer path;
the folded exact-match query pins the indexed symbol lookup ahead of generation
membership so SQLite does not scan all active files. The normal and qualified search
tiers remain exact/prefix/FTS ranked and covered by live SQLite tests. Writer timeout
errors now identify the bounded operation without increasing the ten-second policy
or treating a late mutation as safely retryable.

Property-style deterministic generators cover canonical repository-relative paths,
typed IDs and byte ranges, opaque cursors, resource URIs, active-generation joins and
serialized response reduction. Real modern and legacy stdio clients additionally
prove closed schemas reject caller roots, malicious repository build/package files
are indexed as data without execution, secret/generated paths stay excluded, FTS
syntax is literal, path/resource attacks fail and complete frames stay within 65,536
bytes. Killed-writer recovery now also proves an explicit memory survives; a forced
migration failure leaves both schema version and pre-existing data intact.

The release binary was copied into a temporary package and measured on generated,
authorized Rust corpora. At 100k files it indexed 100k symbols with no failures in
110.237 s (907.137 files/s) at sampled 535,855,104-byte peak RSS. Modern and legacy
startup p95 values are 13.700 ms and 6.910 ms, status p95 while indexing is 0.143 ms
and real cancellation reaches terminal state in 385.353 ms. Warm exact lookup p95 is
60.323 ms, passing the 100 ms target. Ten polling-fallback watcher edits on the 10k
fixture have a measured post-debounce reconciliation p95 of 214 ms, passing the
500 ms target; event-to-activation p95 is separately recorded as 2.327 s because
the controlled runner uses a two-second polling interval. Full-generation resolution
remains in the targeted path, while event hints avoid a redundant complete source scan.
Manual index requests now queue behind watcher-owned reconciliation instead of being
spuriously rejected as writer-busy. Terminal workers are pruned before admission,
while another live manual request remains retryable-busy.

The extraction fixtures prove 157/157 required declaration anchors across the nine
language/dialect adapters and zero occurrences across 23 explicit forbidden checks.
The reviewed parser golden now supplies exhaustive kind/name/byte-range labels for
every emitted fixture declaration, and the prior 35-site per-language resolution
precision/recall metrics still pass. A five-file, source-hash-locked corpus of 6,083
bytes from this authorized project adds exhaustive Rust labels; the generated 100k
corpus remains capacity evidence rather than a semantic sample.

An isolated nightly toolchain built and ran parser extraction, boundary decoder and
tool-argument fuzz targets for five minutes each. The runs executed 50,075,
8,004,978 and 8,630,950 units respectively with no crashes or timeouts. Cargo-audit
and cargo-deny pass the locked graph and explicit source/license policy. These are
bounded bug-finding and supply-chain checks, not proofs of native grammar safety or
legal approval.

## Phase 15 decisions

The macOS ARM64 package is built from `Cargo.lock` with the pinned Rust toolchain and
`--release --locked --offline`. Its deterministic tar/gzip stream fixes entry order,
mtime, ownership and modes. The 25,013,888-byte Mach-O links only CoreFoundation,
CoreServices, libiconv and libSystem; SQLite, grammars and query assets are embedded.
The 5,484,074-byte archive has SHA-256
`57e64086ba5fb066f0fd79f1a2aeda71080e7cb68e8a7c8a7ca023327485c427`.

`integrate codex` uses exact `toml_edit` 0.25.15, canonical absolute binary/root
paths, a comment-based ownership marker, surgical dry-run output and explicit
backed-up apply/remove. It sets `required = false` and keeps the normal Codex timeout
values. Malformed TOML, config symlinks, non-executable/relative paths and unowned
name collisions fail without replacing the config. Removal does not purge source,
indexes or project memories.

The archive test runs after extraction into a Unicode/space path with an empty PATH,
indexes one source in each required language, exercises both MCP protocol eras,
checks follower reads, corrupt-database discovery/tools-list and EOF, then performs
install/remove against a disposable config. The installed Codex CLI parses that
entry. A model-backed Codex session remains unexecuted because it would transmit
fixture content and no explicit authorization for that transmission was given.

## Current limitations and deferred targets

Phase 15 is complete for the explicitly supported macOS 27.0 ARM64 target. Linux x64
and Windows x64 MSVC native path, junction, permission, watcher, process, lock and
package tests are not run and those targets are not supported. Their exact later
qualification workflow is `prompts/14-linux-windows-native-validation.md`.

The measured Phase 14 100k warm exact lookup and 10k post-debounce single-file edit
targets still pass. Phase 15 produces an unsigned/unnotarized 25,013,888-byte Mach-O
that links only expected macOS system libraries. Native grammar C code and repository-read
TOCTOU remain documented residual risks; no sandbox, hostile-co-tenant, representative
multi-repository accuracy or cross-platform guarantee is claimed. The archive has not
been publicly uploaded and the model-backed installed-Codex recipe remains manual.

## Update rule
Append per-phase evidence and exact next step. Mark a phase blocked when an essential
check cannot run. Do not change pending to complete based only on generated files.
