# Architecture — implementation contract

Status: Phase 14 implements the workspace, typed core baseline and production stdio
lifecycle, safe repository/storage foundation, all required syntax extractors and a
bounded persistent indexing/job pipeline with atomic generations. It also implements
evidence-preserving lexical/import resolution plus generation-bound ranked retrieval,
fresh source reads, bounded graph use cases and deterministic context assembly.
All thirteen code/index operations plus explicit memory search are exposed by
default; trusted write opt-in exposes the remaining two memory mutations.
Opt-in owner-only watching, bounded event coalescing and periodic scan/hash
reconciliation are implemented. Four typed resources and three static client prompt
templates are implemented. Hardening/evaluation is complete for the supported macOS
ARM64 target. Packaging, Codex integration and deferred Linux/Windows qualification remain.

## Product boundary
CodeAtlas indexes source syntax and selected repository structure into a persistent
local graph and text index. Codex queries it for compact, evidence-backed context.
The MCP server is not an LLM, a compiler, a general shell, or an autonomous editor.
The indexed repository is read-only. Index/memory writes affect only application data.

Default distribution: one native executable `codeatlas`. The currently supported and
qualified target is macOS ARM64. Windows x64 MSVC and Linux x64 remain deferred,
untested targets and must not be advertised as supported until their native prompt
passes. macOS x64/Linux ARM64 artifacts also require separate execution evidence.
Runtime has no Rust, Dart,
Node, JVM, .NET SDK or Go installation requirement. Building native grammars and
bundled SQLite does require a C/C++ build toolchain as appropriate to dependencies.
“Single executable” does not mean no libc/system-ABI dependencies on every platform.

## Technology decisions
| Concern | Choice | Reason / caveat |
|---|---|---|
| Application | Rust, stable pinned by phase 00 | Memory-safe application code; native grammar FFI remains trusted |
| MCP | Official rmcp with Tokio | Reuse protocol framing, negotiation and version behavior |
| Models | serde + SDK-compatible schemars | Typed inputs/outputs, generated JSON Schemas |
| Syntax | tree-sitter + compiled grammar bindings | Per-language query adapters; no parser download at runtime |
| Persistence | rusqlite bundled SQLite + FTS5 + WAL | One transactional store for graph metadata and lexical retrieval |
| Traversal | ignore + globset | Respect ignore files and bounded source discovery |
| Change watching | notify | Debounce, event reconciliation, optional polling fallback |
| Fingerprints | blake3 | Content/config/extractor fingerprints; not authentication |
| CLI/config | clap + serde TOML + toml_edit | Validation and surgical Codex configuration edits |
| Git metadata | gix, narrowly enabled | Read HEAD/worktree metadata without running repository hooks |
| Concurrency | bounded Tokio channels + blocking parser workers | Protocol responsiveness and backpressure |
| Logs/errors | tracing to stderr, thiserror | No source text or secret-bearing input in logs by default |
| Tests | insta, proptest, tempfile, criterion, cargo-fuzz | Golden contracts, invariants, measurements, native boundary fuzzing |
| Automation | Rust xtask + GitHub Actions | Reproducible checks across target OSes |

Phase 00 verified and pinned the Rust, rmcp, Tree-sitter/grammar and bundled SQLite
subset in `config/dependency-lock.json`. Phase 02 promoted rusqlite, gix, ignore,
fs4, BLAKE3 and the Unix no-follow support dependency into the production lock.
Phase 03 promoted Tree-sitter 0.27.0 and the seven pinned grammar packages into the
production lock. Phases 04–11 add no new runtime dependency; they implement Rust/Go,
C#/Java, JS/JSX/TypeScript/TSX and Dart adapters over the pinned grammars. Other
planned libraries remain design selections until their implementation phases compile
and test them. Phase 12 promotes notify 8.2.0 into `ca-cli`; native events and the
PollWatcher are infrastructure hints and never replace the repository scanner.
Phase 13 adds no third-party dependency; it uses the pinned rmcp resource/prompt
surface and schema-v5 memory tables. Phase 14 adds schema-v6 indexes and no
production dependency. Its exact `libfuzzer-sys` declaration is isolated under the
workspace-excluded `fuzz/` package and lockfile. The separate nightly fuzz toolchain
built and ran all three five-minute targets on macOS; cargo-audit and cargo-deny
passed the locked dependency graph and explicit supply-chain policy.
Do not add Tantivy, a vector DB, or an embedded model until a measured requirement
justifies a separate ADR. SQLite is the sole v1 consistency boundary.

## Workspace shape
```text
crates/
  ca-core/        # IDs, ranges, facts, errors, limits, port traits
  ca-languages/   # registry + one module per language/dialect + embedded .scm queries
  ca-storage/     # migrations, writer actor, read snapshots, SQLite/FTS implementation
  ca-engine/      # features: repository/indexing/resolution/search/graph/context/memory
  ca-mcp/         # rmcp adapters, schemas, resources, prompts, error translation
  ca-cli/         # binary composition, serve/index/doctor/config/integrate
xtask/           # explicitly implemented developer/test/release commands
```
Dependency direction: core <- languages/storage; core <- engine; adapters are wired
at the CLI composition root. Engine depends on ports rather than concrete SQLite or
MCP types. ca-mcp delegates business decisions to engine; it does not contain SQL.
Avoid a crate per tiny function or giant utilities crate.

The phase-00 spike remains separately excluded at `spikes/compatibility`; its own
lockfile and evidence were not reused as production execution evidence. Phase 01
promoted the pinned Rust/rmcp/serde/schemars choices needed by the status-only server.
Phase 02 promoted SQLite and repository-boundary dependencies. Phase 03 promotes
Tree-sitter only into `ca-languages`; `ca-core`, `ca-storage`, `ca-engine` and
`ca-mcp` remain free of parser dependencies at this boundary.

## Main data flow
```text
authorized root
  -> bounded ignore-aware scan
  -> immutable source snapshot + content hash
  -> language registry -> parser -> queries -> owned syntax facts
  -> Phase 08 immutable candidate generation
  -> Phase 09 full-generation lexical/import resolver
  -> evidence-labelled containment/import/reference/call graph
  -> staged SQLite generation -> atomic active-generation switch
  -> bounded search/context/protocol use cases
  -> optional notify hint/debounce or periodic reconciliation -> repeat scan/hash
```
Persist owned facts, not Tree-sitter Node references or parser pointers. Parser
instances and compiled query objects remain worker-local. Phase 04–07 source results
own structural observation IDs, ranges, scopes, containers, roles and limitations;
calls are deliberately unresolved; C# partial groups remain evidence hints, Java
method references remain non-call references, JSX tags remain references, and Dart
constructor-like/Flutter-style calls remain syntax evidence without a widget tree.
Phase 08 runs each `ParserWorker` on a bounded worker, uses a bounded source queue,
persists results in batches, and shares cooperative cancellation with native parse
and query callbacks. Grammar, query and extractor fingerprints are returned with
every parse and all participate in reuse decisions with content/config fingerprints.
Phase 09 then resolves every candidate generation against its exact immutable
membership. Engine domain models remain independent of SQLite and Tree-sitter;
the CLI adapter maps owned observations into resolver inputs and maps graph output
to one writer-owned transaction. Activation refuses generations without recorded
resolver metadata.

The resolver implements deterministic lexical scope chains, narrow same-file and
module/import rules, aliasing and bounded re-export traversal. It reads only root
`go.mod` and `pubspec.yaml` declarations as inert data. Dynamic targets, missing or
unsupported mappings, overloads and receiver dispatch preserve unresolved/candidate
labels and evidence. Graph traversal is breadth-first, cycle-safe and bounded by
depth, node and edge caps; candidates require explicit opt-in.

## Lifecycle and multi-client behavior
`codeatlas serve --root <absolute-authorized-root>` is client-owned and exits on
transport EOF/termination. No detached child daemon and no orphan service.

Transport is available before the repository is opened. `repository_status` can
report `not_opened`, `opening`, `ready`, `read_only_follower`, or `degraded`.
Opening/migrations are lazy and bounded outside discovery. Index creation is an
explicit tool or CLI operation; `auto_index` and `watch` default to false.
`serve --watch` starts no repository work during discovery. It becomes active only
after the process acquires writer ownership through an explicit index call.

The Phase 08 CLI `index` operation remains synchronous. Phase 11 adds an asynchronous
MCP adapter that durably prepares a job, returns its ID and runs the same engine use
case on a retained background worker. Active storage/cancellation handles make
job_status and cancel_job responsive; EOF cancels and joins workers. These are
application jobs, not standardized MCP Tasks. `status` and `doctor` inspect actual
storage state; `serve` still performs no traversal, migration or parser loading
during handshake/discovery.

Each root/worktree has one write-owner lock held by an OS file handle, not by a
PID-file convention. The owner has a dedicated SQLite writer thread and can watch.
Other processes query the same local database. Mutating calls in followers return
`WRITER_BUSY` with a retry hint; there is no hidden remote queue. A follower can try
to acquire the released owner lock on a later mutation. An active owner is never
killed or stolen based on an old timestamp. Process death releases the OS lock.

The Phase 12 owner retains its storage handle for the session and owns at most one
watcher coordinator. notify callbacks use a bounded nonblocking queue; overflow is
observable. Debounce coalesces editor save/rename bursts, and a periodic timer still
runs the full ignore-aware scan and content-hash comparison when events are missing.
Every watch job uses changed-file full parsing and full-generation graph resolution
behind the same serialization gate as manual jobs. EOF cancels active jobs, wakes and
joins the watcher, joins workers and only then releases the owner handle.

Phase 13 keeps memory rows outside generation ownership. Each note has explicit
kind, provenance, scope and revision, with exact optional path/hash/symbol evidence.
The writer actor validates evidence against the active generation and applies
optimistic upsert/delete transactions; reads recompute verified/unverified/stale
state. Reindex and generation GC cannot silently delete notes. `--memory-write` is a
trusted process-start policy: it hides mutation routes when false and is checked
again in the backend when true. Repository configuration is never consulted for it.

Static status/map resources and typed symbol/memory resource templates delegate to
the same backend operations as tools. URI parsing accepts no filesystem scheme,
encoded separator, traversal or arbitrary root. Prompt templates only return client
instructions; they invoke no model and label repository/memory content as untrusted.

If no DB exists and another process owns creation, status stays available and
query calls return `INDEX_NOT_READY`. No client waits indefinitely. OS locks,
SQLite busy timeouts, writer ownership and read-only WAL opening are tested on
Windows and macOS, not assumed from Linux behavior.

## Feature modules and ports
| Feature | Responsibilities | Ports / key boundary |
|---|---|---|
| repository | root authorization, safe reads, worktree ID, discovery | SourceReader, RepoMetadata |
| indexing | jobs, scan/parse/stage/activate, cancellation | FileScanner, LanguageRegistry, IndexStore |
| resolution | scopes, imports, candidates, edge evidence | IndexStore resolution input/writer ports |
| retrieval | exact/name/prefix/FTS ranking, cursors, outlines and fresh reads | RetrievalStore + SourceReader |
| graph | bounded references/callers/callees/impact traversal | RetrievalStore graph reads |
| context | deterministic evidence selection and serialized-output budgets | retrieval/graph use cases |
| memory | explicit notes, provenance, stale-evidence validation | MemoryStore, evidence validator |

Ports should be minimal and driven by use cases, not a universal CRUD repository.
Do not put DB structs, serde_json::Value, or protocol session handles in core facts.

Phase 10 implements `ca-engine::retrieval` as the use-case boundary. One pinned
`ReadSnapshot` adapter supplies generation-scoped facts; SQLite never owns cursor,
ranking-policy, source-freshness or response-envelope decisions. The engine binds
cursors to repository/generation/operation/query, verifies source through
`SourceReader`, traverses graphs with explicit depth/node/edge/deadline caps and
accounts for the final structured-content plus text-fallback serialization. Phase 11
adds thin rmcp handlers and a CLI backend port without duplicating these policies in
transport code. All blocking repository/storage work is dispatched off the async
protocol executor.

## Identity, scope and positions
Worktree identity is local: derive from losslessly represented canonical root and
worktree metadata. Two Git worktrees MUST have distinct databases even when remote
URLs match. Moving the root creates a new local identity in v1; explicit migration
is a separate feature. Gitless directories are supported. Never blindly lowercase
paths; case behavior depends on the actual filesystem.

A symbol has a declaration identity and optionally a logical group identity.
Use language, relative path, enclosing declaration, kind, name, normalized source
signature and a deterministic disambiguator. A source line number alone is not an
identity. Partial C# declarations are distinct declaration records grouped only
with evidence. Renames need not preserve IDs; memory rebinding must be explicit.

Public byte ranges are zero-based UTF-8 offsets with an exclusive end. Human line
numbers are one-based; explicitly named byte columns are zero-based. Internal
Tree-sitter points are never silently treated as UTF-16 LSP positions. v1 accepts
valid UTF-8 and retains a UTF-8 BOM as the first three source bytes, so first-line
columns and all byte ranges continue to address the original input. Invalid encoding
is rejected before native parsing. UTF-16 and non-UTF-8 paths need an explicit tested
policy, not lossy conversion that can cause collisions. Preserve CRLF/newline bytes
in fingerprints and calculate byte columns from unmodified input.

## Extensibility
Add a language by implementing a registry entry, grammar provider, capture queries,
ownership/normalization adapter, resolver rules, capability matrix and fixtures.
No global switch that requires changing every feature. Do not confuse a compiled
adapter architecture with an untrusted dynamic plugin system.

Optional phase 16 adds framework extractors. Optional phase 17 adds explicitly
approved LSP enrichment through a new port. The seven Tree-sitter adapters remain
functional without either extension. No semantic feature is a mandatory startup
or indexing dependency.
