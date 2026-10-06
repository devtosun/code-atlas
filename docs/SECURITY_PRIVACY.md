# Security and privacy acceptance policy

## Trust boundaries

Review-repair policies: rejected UTF-8, NUL/binary and oversized supported source
make an index scan incomplete, preserving the healthy active generation. Exclusion
and confirmed deletion remain separate. Field/member selectors retain receiver
expressions and are candidates/unresolved without type inference, never bare-name
bindings to shadowing locals. Generation history is active plus predecessor;
long-lived readers may retain WAL pages and notes are independent of that GC.
Malformed Codex TOML diagnostics expose only file path and line/column, not source
excerpts or the parser's source-bearing error chain. MCP ingress, ID and complete
response limits are described in `docs/MCP_CONTRACT.md`; unsafe frames close the
connection. None of these changes authorizes executing repository content.

1. Trusted application binary and compiled dependencies, including native grammars.
2. Explicit user-selected root and trusted local runtime policy.
3. Untrusted repository source, comments, documents, manifests, symlinks and filenames.
4. Untrusted tool arguments/resource URIs/cursors and externally authored notes.
5. MCP client/model, which can receive retrieved code under the user's chosen client policy.

“No external API inside the indexer” does NOT imply code cannot leave the device:
Codex or another remote model may receive tool output. Explain this at integration,
and support strict includes/excludes and bounded snippets. Do not advertise a local
index as end-to-end local AI inference.

## File access
Authorize roots before reading. Do not use string prefix checks (repo vs repo2).
Reject absolute tool paths, `..`, NUL, disallowed schemes, symlink/junction escapes
and cross-root IDs. Resolve paths with platform-aware components and safe handles
where feasible to address time-of-check/time-of-use races. Revalidate at actual read.
Default to not following symlinks. Root itself may be canonicalized once under
trusted local authorization; a tool cannot change the root later.

Reject/diagnose invalid UTF-8 contents and nonrepresentable path identities rather
than silently conflating files. Bound path length/segments and handle Windows drive,
UNC/device paths and junctions explicitly. Native path tests are mandatory.

Respect .gitignore, .ignore and .codeatlasignore. Hard-deny credentials and runtime
index paths even if broad include patterns exist. Defaults exclude .git, node_modules,
target, bin, obj, build, dist, .dart_tool, .gradle, vendor and generated/minified files.
Allow reviewed project-specific exceptions for source directories named build/vendor;
hard secret denies cannot be overridden by untrusted repository config.

Default hard-deny classes: .env variants, private key/certificate containers,
credential files and explicit user exclusions. A heuristic secret detector is defense
in depth, not a proof of secret-free output. Normal source code may contain secrets.
Treat non-source config/doc ingestion as a separately approved feature.

Phase 02 implements this boundary with an allowlist for the required language source
extensions, hard secret/build/minified exclusions and three ignore-file formats.
Roots are canonicalized once; each requested path is component-validated, link-like
components are rejected, and the final file is opened no-follow where the platform
supports it. The opened file identity and length are rechecked and its resolved path
must remain under the authorized root before bytes are returned. Windows reparse
points are explicitly rejected in code, but junction behavior has not yet run on a
native Windows host.

Portable Rust does not provide a race-free directory-descriptor traversal API on all
three target platforms. Phase 02 therefore still has a narrow directory-enumeration
TOCTOU window if a hostile local process with write access continuously swaps an
intermediate directory. Source bytes are separately revalidated through the safe
reader before they are returned. The authorized root and normal local worktree are
assumed to be controlled by the current user; stronger hostile-co-tenant isolation
would require platform-specific handle traversal or a separately designed worker.

## Repository/config instructions are data
Source comments, markdown snippets, memory contents and returned instructions are
untrusted context, never developer/system instructions. Label evidence as data and
retain origin. The server cannot guarantee the client will resist every prompt
injection; document the residual risk. Do not “sanitize” by silently changing code
that the user expects to inspect. Preserve content and annotate boundaries.

Repository config cannot enable network traffic, shell commands, external LSP
processes, broader roots or memory writes. Privileged options require trusted CLI or
user config. Parse manifests as data only. Do not evaluate MSBuild, Gradle, Cargo,
npm, Dart build scripts or custom tsconfig loaders.

## Resource exhaustion and native code
Limits cover bytes/file, files/job, queue slots, source buffers, parser concurrency,
parse/query time budget, captures/file, graph nodes/edges/depth, FTS query length,
response bytes, memory length/count, DB growth and log rotation. Test oversized,
malformed and deeply nested input. Stop scan when queue backpressure requires it.

Cancellation is cooperative for in-process native parsing. A hard native crash or
noncooperative scanner cannot be neutralized by Rust panic handling. Do not claim
sandbox-grade isolation. A future worker subprocess design must be an ADR and cannot
reintroduce a mandatory startup daemon. This release accepts trusted compiled grammar
code and reduces input risk through fuzzing, version audits and resource bounds.

Phase 08 uses a bounded source queue and a capped worker count; parser instances and
compiled queries are worker-local. Source is safely reread and content-hashed before
reuse, then the same owned bytes are passed to extraction. Durable cancellation is
polled while queueing and awaiting workers and is propagated into parser/query
callbacks. Traversal, invalid-encoding, oversized-file and storage failures abort the
candidate generation rather than inferring deletions or displacing healthy active data.

Phase 09 reads only root `go.mod` module and `pubspec.yaml` package declarations
through the same authorized no-follow source boundary and treats them as inert UTF-8
data. Resolution executes no compiler, SDK, package manager, build script, module
loader or repository instruction. Re-export and graph traversal enforce explicit
depth/node/edge/candidate caps; unsupported mappings and limit exhaustion remain
diagnostics rather than triggers for broader filesystem or process access.

Phase 10 accepts only bounded typed retrieval inputs. SQL values and FTS expressions
remain parameters; user text is converted to quoted alphanumeric identifier tokens,
not accepted as FTS or SQL syntax. Every read stays on one pinned generation.
Opaque cursors are length/type checked and rejected when their repository,
generation, operation or query fingerprint differs. `read_code` and context snippets
reuse the authorized no-follow reader, reapply current ignore/secret policy and
compare the live BLAKE3 hash with indexed evidence, so edited, deleted and newly
excluded files are explicit failures rather than stale disclosure. Final response
limits count JSON escaping and the duplicated text fallback; reduction removes whole
items or lines and never slices serialized bytes.

Phase 11 exposes these reads through closed generated MCP schemas and keeps the
authorized root solely in startup state; no tool accepts a root or arbitrary
filesystem path. Enum/range/ID validation happens before repository work where
possible, and repository-bound storage, generation checks and cursors reject foreign
objects. Application failures are structured `isError` data, while malformed
protocol messages remain rmcp errors. The final MCP result is serialized against a
transport reserve, so a caller cannot bypass the byte cap with escaping or fallback
duplication. Indexing runs on a retained worker with durable job state and
cooperative cancellation; EOF cancels and joins it. The MCP Tasks capability is not
advertised.

Phase 12 keeps watching behind the trusted `serve --watch` startup option. Repository
files cannot enable watching, polling, broader roots, network access or memory writes.
notify observes only the already-authorized canonical root, does not follow symlinks,
and feeds paths only as bounded hints; the existing scanner and safe reader reapply
component containment, ignore and secret rules before hashing or parsing. Queue
saturation and backend errors are visible, while periodic full scan/hash
reconciliation prevents event loss from becoming silent permanent staleness.
Application data remains in the local data directory outside the watched source tree,
so index writes do not create a feedback loop. Mounted/network sources are explicitly
reported with degraded event-delivery guarantees; PollWatcher may increase local I/O
but neither backend executes repository content or changes system watch limits.

Phase 13 makes memory reads available but keeps mutation behind the trusted
`serve --memory-write` startup flag. When false, mutation routes are absent from
tools/list; the backend checks the same policy independently. No repository file,
comment, manifest or `.codeatlasignore` setting is consulted for this privilege.
Writes enforce bounded text/metadata/count/evidence and exact optimistic revisions.
Supplied evidence must match the active path/hash and optional immutable symbol ID;
later changes are surfaced as stale and never rebound by name.

Memory text, authorship fields, prompt arguments and repository excerpts are
returned as labelled untrusted data. The synthetic injection fixture preserves text
asking for secret upload while the generated prompt explicitly treats it as data;
the server neither executes the text nor claims it can force a remote model to obey.
Resource parsing accepts only the four `codeatlas://repo/...` forms and rejects
`file://`, encoded/path-like IDs and traversal. Status/map/symbol/memory reads reuse
the same repository-bound operations and byte cap rather than opening filesystem
paths from URIs. The local SQLite database remains plaintext and explicit notes may
be sent to a remote client model under that client's policy.

Phase 14 exercises the boundary through real modern and legacy stdio clients. Closed
tool schemas reject caller-supplied roots; path tools reject traversal, absolute,
repeated-separator, generated, secret and Windows-form paths; resource parsing rejects
traversal, percent-encoding and `file://`. Malicious FTS-looking text remains a
parameterized literal query. Synthetic `build.rs` and `package.json` inputs contain a
marker-writing payload, but indexing treats both as inert bytes and no marker is
created. `.env` and generated `target` source remain excluded. Every emitted frame in
the adversarial run parses as JSON and remains at or below 65,536 bytes.
An independent live-process check after modern discovery/tools-list finds no network
socket rows with `lsof -i`; the server also reports `runtime_network=false`.

Deterministic generated properties cover canonical relative paths, typed IDs/ranges,
opaque cursor fingerprints, resource URIs, active-generation joins and response
reduction. Killed-writer recovery proves both the prior active generation and an
explicit note survive; a forced migration failure rolls back schema metadata and
pre-existing rows. Live cancellation observes an active job and reaches terminal
`cancelled` in 385.353 ms rather than only checking a pre-work fast error.

The separate nightly harness built and ran parser extraction, path/URI/cursor
decoding and all tool-argument decoders for five minutes each on isolated corpora.
It executed 50,075, 8,004,978 and 8,630,950 units with zero crash artifacts or
timeouts. Cargo-audit found no vulnerability across 294 locked dependencies and
cargo-deny passed the explicit advisory, ban, license and source policy.

These tests do not create a sandbox or prove memory safety. Trusted statically linked
native grammar code can still fail below Rust's panic boundary, and the documented
component-walk TOCTOU window remains for a hostile same-user process. Normal source
files can contain credentials outside the hard deny classes. A remote MCP client/model
can receive returned source or notes. Windows junctions and Linux/Windows native
behavior remain untested and unsupported until the deferred native matrix passes.

## DB and configuration
No arbitrary SQL/PRAGMA tool and no user-controlled SQL identifiers without strict
enums. Quote FTS literal queries separately from SQL parameterization. Use owner
permissions, transaction-backed migrations, WAL-safe backups, and a bounded lock
strategy. A query-only follower must never auto-upgrade or rewrite a corrupt DB.

Integration is opt-in, dry-run first. Back up an existing Codex config; preserve
comments, unrelated settings and all other MCP definitions using toml_edit. Never
set `required=true` for this optional server. Never lower global execution policy,
request admin unnecessarily, kill unrelated processes, or pipe remote scripts to a
shell. No auto-install dependencies at runtime. Uninstall must leave source and
personal notes intact unless separately explicitly purged.

Phase 15 implements this policy for macOS ARM64. The CLI requires absolute canonical
root/binary paths, rejects a non-executable binary, config symlink, malformed TOML and
an unowned `mcp_servers.codeatlas` collision. The ownership marker is a comment on the
table rather than an unknown Codex key. Dry-run renders only that table's diff. Apply
creates a restrictive sibling backup and temporary file, preserves the original mode,
then uses same-directory atomic rename. The entry is optional (`required = false`) and
uses Codex's normal 10-second startup and 60-second tool limits. Removal deletes only
the marked table and does not touch source, indexes or project memories.

The native package includes this privacy guide because a real Codex model session can
transmit retrieved fixture/source text under the user's client policy. Automated
Phase 15 tests use direct local subprocess clients and a disposable installed-Codex
configuration parser check; they do not start a model-backed session or read/write
the real user config.
