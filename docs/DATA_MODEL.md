# Persistence and consistency contract

## One SQLite database per authorized worktree
Keep the database under the platform's local application data directory, outside
the source repository and outside network shares. The resolved path and owner mode
are visible through `doctor`. Apply owner-only permissions where supported; document
Windows ACL behavior. Do not promise encryption at rest: SQLite plaintext stores can
contain names, signatures, snippets and explicit memory text.

## Logical tables

Review repairs ship and test schema version 7. It retains the Phase 08 indexing model
and adds generation resolver version/count metadata, `external_nodes`,
`resolved_edges` and `resolution_diagnostics`. A unique generation/file-version
membership key supports composite foreign keys that prevent graph rows from naming
local source or target versions outside their generation.

Schema v6 adds `generation_files(file_version_id)` for bounded orphan-version
cleanup and `symbols(lower(spelling))` for folded exact/prefix retrieval. These are
derived access paths only; generation identity, immutable rows and activation
semantics are unchanged.

Schema v7 makes file-version identity `(file_id, content_hash, analysis_key)`.
`analysis_key` is a versioned length-framed encoding of language/dialect, grammar,
query, extractor and configuration fingerprints; separators cannot alias keys.
The v6 unique-key column is renamed in a transaction, not rebuilt or dropped.
Legacy extractor fingerprints are preserved in the new `extractor_hash` column,
their analysis keys are namespaced `legacy-v6:`, and their per-version `config_hash`
is empty to force revalidation. IDs, FKs, facts, FTS rows and notes survive.
Reusing a complete identity links membership only; it never appends facts.
The v7 `(file_version_id, spelling)` symbol index bounds same-name FTS joins to the
selected file version. Higher-tier direct matches are excluded before FTS scoring.

The full logical model is:
- `meta`: schema version, active generation ID, root identity, limits fingerprint.
- `generations`: ID, parent, status, start/end, observed HEAD/dirty marker,
  config/extractor hashes, coverage, failure summary and scan-complete flag.
- `files`: stable local file identity + lossless relative path representation.
- `file_versions`: content hash, language/dialect, grammar/query version,
  parse status, byte length, source encoding and observed metadata.
- `generation_files`: (generation, file) -> immutable file version membership.
- `symbols`: file version, declaration ID, logical key, container, name, kind,
  signature, visibility, doc excerpt, byte/line range and extraction evidence.
- `scopes`, `imports`, `references`, `call_sites`: immutable syntax observations
  belonging to file versions. A call site exists even if its target is unknown.
- `resolved_edges`: generation-scoped source observation, target declaration or
  external target, relationship kind, resolution category, candidate count,
  reason and evidence. Rows also retain the source category/range/spelling,
  containing symbol, rule/resolver version and applied limits. The generation must
  contain every local source and target version.
- `external_nodes`: generation-scoped static module/package observations that are
  not fabricated into local symbols.
- `resolution_diagnostics`: bounded generation-scoped cycle, unsupported-rule,
  missing-module and resolver-limit evidence.
- `search_documents` + `symbol_fts`: immutable search rows per version, scoped
  through generation membership. Derived content includes identifier tokens split
  at camel/snake/punctuation boundaries; exact name/path columns remain available
  for higher-priority deterministic tiers.
- `jobs`: durable lifecycle/status/progress/error summaries, owner instance ID.
- `memories`, `memory_evidence`, `memory_fts`: explicitly authored notes independent
  of transient index generations, with evidence hashes and timestamps.

Schema v5 gives every memory a decision/convention/pitfall/task kind, monotonically
increasing revision, author, origin, scope, created/updated timestamps and bounded
text. Evidence rows retain the exact repository-relative path and content hash plus
an optional immutable symbol observation ID. Evidence has no foreign key to a
generation: this is intentional so reindex, generation GC and rebuild cannot erase
the user's note. Reads compare each row with the active generation and report
`verified`, `unverified` or `stale`; a symbol is checked by exact ID and is never
rebound by matching a renamed declaration's name.

Phases 04–07 produce the Rust/Go, JS/JSX/TypeScript/TSX, C#/Java and Dart source
observations stored in `symbols`, `scopes`, `imports`, `references` and `call_sites`,
including stable observation IDs, owning scope IDs and extractor fingerprints.
JS/TS import observations include ESM imports/exports/re-exports, reviewed CommonJS
forms and explicit type/value roles; JSX tags remain reference observations. Phase 08
maps these values into immutable file versions. Phase 09 derives graph rows from those
observations without mutating the immutable syntax facts. `target_id` on the source
observation remains syntax data; authoritative bindings are generation-scoped edges.
C# partial
declarations remain separate observations with an unconfirmed grouping hint;
C#/Java callable signatures preserve overload and nested-type identity. Member
receivers are stored as source expressions with an explicit no-type-inference
limitation, and Java method references remain non-call reference observations. Dart
library/import/export/part relationships remain immutable syntax observations;
constructor declaration kinds stay distinct, capitalized calls remain constructor-
like rather than resolved, and Flutter-style nesting does not create semantic widget
or navigation edges.

Schema-v4 activation rejects a candidate until resolver metadata has been written.
The writer validates graph caps, observation identity, source membership and target
membership before one transaction replaces that candidate's prior graph. A local
edge can target only a symbol whose exact immutable file version is selected by the
same generation. Candidate and unresolved sites are stored rather than discarded;
query APIs exclude candidate edges unless a caller explicitly opts in.

Use foreign keys, unique constraints, parameterized SQL, indexes for incoming and
outgoing edges, and transactional FTS updates. Choose either documented external-
content FTS triggers or explicit synchronization, not both accidentally. Migration
and integrity tests prove the schema rather than relying on this prose.

## Generation algorithm
1. Acquire the write-owner lock without blocking the MCP transport.
2. Create a building generation; copy only membership from the active generation.
3. Complete ignore-aware traversal. A directory permission error or interrupted
   traversal makes deletion inference unsafe: abort activation and retain active.
4. Hash file contents before deciding reuse; mtime/size alone is not correctness.
   Reuse immutable versions only if content AND extraction/config fingerprints match.
5. Parse changed files in bounded workers from the same immutable bytes that were
   hashed. Recheck observation conflicts before activation and report a dirty view
   when files change during indexing; a worktree snapshot is not a Git commit snapshot.
6. Write new file versions/facts in short batches. Deleted membership is removed
   only after successful full traversal of the relevant scope.
7. Resolve cross-file edges against the candidate generation. Phase 09 performs full
   graph re-resolution even when parsing is incremental; the rename/delete/export
   corpus proves equivalence to a clean full index before activation.
8. In one short transaction validate invariants, mark the generation active and
   update the active pointer. No partially built generation is returned by queries.
9. Mark the job complete; emit optional negotiated notifications only afterwards.

## Watch reconciliation

Phase 12 adds no schema migration. An enabled owner session treats filesystem events
as bounded hints and schedules ordinary durable incremental jobs. A bounded callback
queue records saturation, debounce collapses editor save/rename bursts, and periodic
reconciliation reruns the complete ignore-aware scan even if the native event stream
is silent. The scanner re-stats and re-hashes source paths; event kinds and mtimes are
never applied directly to database membership.

Changes to source, ignore files, repository metadata or compiled extraction policy
all flow through the existing fingerprint checks. Changed files are fully reparsed;
unchanged immutable versions may be reused only on exact content/config/grammar/query/
extractor fingerprints. Full graph re-resolution removes obsolete incoming as well
as outgoing bindings before candidate activation. Queue overflow, watcher errors or
process death never mutate the active generation directly, so the next periodic or
post-restart index converges through the same generation algorithm.

Syntax errors can yield partial facts plus parse diagnostics. Never retain a previous
file's obsolete symbols and label them fresh. A failed file version contains an
explicit failure/coverage record. Activation policy for per-file parse errors is
`ready_with_warnings`; traversal/storage/cancellation failures do not activate.

If a job is cancelled or crashes before activation, abandon its generation. After
restart mark the old owner's queued/running jobs interrupted. Do not silently resume
a risky operation or report it successful. GC removes abandoned/unreferenced versions
without touching active data or persistent notes. Completed history is bounded to
the active generation and its immediate predecessor, plus an in-progress candidate.
GC detaches expired parent links and removes superseded generations and orphan
versions/FTS rows. Existing WAL read transactions stay pinned; public cursors still
require the active generation and return `STALE_CURSOR` after activation. This is
a generation-count retention policy, not a byte/disk quota or a timed cursor lease.
Long-lived pinned readers can retain WAL pages; notes are never evicted by this GC.

Memory writes run on the same bounded writer actor and use `synchronous=FULL`.
Creating a note requires a fresh ID; updating or deleting an existing note requires
the exact expected revision. The transaction validates all supplied evidence before
replacing the note and its evidence rows. Limits are 16 KiB text, 10,000 notes per
repository, 32 evidence rows and 8 KiB aggregate evidence per note, 256-byte search
input/metadata fields and 100
search results. Memory FTS is trigger-maintained transactionally. Search excludes
stale notes by default and structurally drops whole results to meet the MCP byte cap.

## Read transactions and pagination
Pin one generation per use-case invocation through a short read transaction.
All symbol, FTS and graph joins must include that generation. Never read the active
pointer repeatedly during one response. Cursors contain operation + repository +
generation + normalized query/filter fingerprint + sort key + format version;
validate lengths/types before reuse.
Retain at least the active and previous generation for a documented cursor window,
subject to configured storage limits. Return `STALE_CURSOR` if its generation was
collected; never silently continue on a different snapshot.

Search evidence may describe an indexed snapshot. `read_code` and snippet-bearing
responses read the currently authorized file and verify its hash before treating
old ranges as current. A mismatch is `CONTENT_CHANGED` with reindex guidance; do
not read arbitrary new bytes at stale offsets. Deleted/now-excluded paths cannot
be recovered through old result IDs or resource URIs.

## Durability and recovery
Enable foreign_keys on every relevant connection. Verify actual bundled SQLite
version and compile options, especially FTS5. WAL requires local same-host storage;
readers and the owner need correct WAL/SHM permissions. Set bounded busy timeouts.
Serialize writes and checkpoints through the owner. Keep read transactions short.

For the default durability policy use `synchronous=FULL` so explicit user notes
receive the stronger available commit durability. A future index-only performance
mode must not weaken note durability silently. Use the SQLite backup API or a
coordinated closed-database backup: copying only the .db while WAL is active is
not a valid backup procedure. Migrations preserve notes and refuse unsupported
future schema versions. Never auto-delete a corrupt database to hide an error.

Atomic visibility, process-crash recovery and power-loss durability are different
claims. Tests must name which is covered. A killed process test does not prove all
power-loss scenarios or every filesystem's atomicity.
