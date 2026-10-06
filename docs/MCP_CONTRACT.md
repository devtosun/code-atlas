# MCP and CLI contract

## Version boundary

Review-repair wire policy: prompt text is capped at 8192 UTF-8 bytes and optional
scope at 4096 bytes, with an additional exact serialized-result check. Request IDs
must serialize to at most 128 bytes (JSON escaping counts). Official rmcp codecs
bound incoming JSON lines at 1 MiB. IDs over the limit or invalid/oversized frames
end the connection before SDK dispatch rather than echoing an unsafe ID. Every
outgoing SDK message is checked including its JSON-RPC wrapper and newline against
65536 bytes. An over-budget SDK error also closes the session cooperatively; it is
never sent as a truncated JSON fragment. These guards do not mix the two eras.

Verified Phase 00 baseline: modern MCP 2026-07-28 and legacy 2025-11-25 using
published official rmcp 3.4.0. The installed `codex-cli 0.154.0` was recorded, but
its negotiated lifecycle was not introspected. Do not equate documentation on main
with the tested release and raw frames in the Phase 00 report.

The modern protocol uses server/discover and per-request version/client metadata;
the legacy lifecycle uses initialize / notifications/initialized. rmcp owns this
translation. Contract tests cover both eras, unknown versions, and malformed input.
Use object-shaped application results for simple backward-compatible schemas.
Do not label only legacy initialize tests as complete current-protocol support.

stdio framing and shutdown are provided by the SDK. In serve mode absolutely no
progress bar, banner, println or third-party logger writes to stdout. Server metadata,
discovery, tools/list and status must remain responsive while the index is unopened,
locked, migrating, cancelled, empty or corrupt.

## Common application result envelope
```json
{
  "schema_version": "1",
  "repository_id": "local-worktree-id",
  "generation_id": "g-0042",
  "status": "ok",
  "data": {},
  "coverage": {"parse_errors": 0, "unresolved_references": 3},
  "warnings": [],
  "truncated": false,
  "next_cursor": null
}
```
Repository/generation can be null for status/job operations before an index exists.
This is the APPLICATION payload inside rmcp's version-correct wire result, not a
replacement for JSON-RPC or an invented MCP envelope. Map application execution
failures to actionable tool results with isError; preserve SDK protocol errors for
malformed protocol messages/unknown methods. Validate generated output schemas.

Default response budget: 65,536 UTF-8 bytes INCLUDING the serialized protocol
wrapper and any TextContent fallback duplication. Default result limit 20, max 200;
default graph depth 2, hard max 8, node cap 500. Limits are configurable downward;
raising hard limits requires policy validation. Bound individual snippets and final
serialization. Never truncate JSON bytes mid-document. Truncate items then reserialize.
A token estimate is not a model-independent guarantee; report its method if used.

## Tools to implement, not placeholders to advertise
| Tool | Key inputs | Behavior / mutation |
|---|---|---|
| repository_status | none | Open/index/owner/language/version/coverage status; no traversal side effect |
| index_repository | mode incremental/full, optional request_key | Queue bounded local application job for already-authorized root; index write |
| job_status | job_id | Durable status/progress/counters/retry hints |
| cancel_job | job_id | Idempotent cooperative cancellation; no forced process kill |
| search_symbols | query, language/kind/path filter, limit, cursor | Exact/prefix/FTS ranked declaration search |
| get_symbol | symbol_id, optional generation | Declaration, signature, evidence, diagnostics |
| find_references | symbol_id, include_candidates=false, limit, cursor | Reference observations/bindings; preserve certainty |
| trace_calls | symbol_id, direction, depth, include_candidates=false | Bounded callers/callees with cycle and candidate flags |
| get_file_outline | relative_path | Bounded declaration outline in selected generation |
| read_code | relative_path, start_line, end_line, expected_hash | Authorized bounded source read; reject stale evidence/escapes |
| get_repo_map | scope, depth, limit | Deterministic structural map; no unsupported narrative claims |
| analyze_impact | symbol_ids or changed relative paths, depth | Reverse dependency/call candidates; not a guarantee of all affected behavior |
| build_context | query, optional scope, max_bytes | Ranked evidence bundle with budget and unresolved/coverage notes |
| search_memories | query, scope, include_stale=false | Explicit notes, clearly distinct from extracted facts |
| upsert_memory | optional memory_id, text, kind, evidence, expected_revision | Opt-in user-authorized note write, optimistic concurrency |
| forget_memory | memory_id, expected_revision | Opt-in destructive note delete; explicit policy and annotation |

`index_repository` cannot select an arbitrary filesystem path. Root authorization
comes from CLI/trusted local config. The same applies to IDs, URIs and job access:
an object from another repository cannot widen the process's authorization.

Advertise tools as implemented per build phase, never fake success/empty placeholders.
Startup-fixed feature/config decisions may hide memory write tools. Schema/annotations
must reflect actual behavior: query tools readOnlyHint true; index/cancel/memory writes
false; forget_memory destructiveHint true; openWorldHint false for default local tools.
These are hints for clients, not server-side authorization. Enforce policy independently.

## Jobs
Job states: queued -> scanning -> parsing -> resolving -> committing -> completed;
any nonterminal state may reach cancelled/failed, or interrupted on owner restart.
A cancel arriving after atomic activation returns already_completed, not a fiction
that the index was rolled back. Store counts and errors, not raw source in logs.

`index_repository` returns promptly with job_id; it does not hold a tool call for a
large repository. `job_status` and `cancel_job` are application tools, NOT a claim
that standardized MCP Tasks is implemented. A future negotiated Tasks adapter can
map onto the job engine independently. Tasks do not survive process termination
unless the recovery design actually implements resumption (v1 does not).

## Resources and MCP prompt templates (phase 13)
Resources: `codeatlas://repo/status`, `codeatlas://repo/map`,
`codeatlas://repo/symbol/<id>`, `codeatlas://repo/memory/<id>`.
Decode/validate URIs through typed parsers. No unrestricted `file://` passthrough.
Resource reads use the same root, hash, budget, policy and generation checks as tools.

MCP prompts: `explain_symbol`, `plan_change`, `investigate_failure`.
These are server-provided client prompt templates, distinct from `prompts/*.md`
(the build instructions in this kit) and `.agents/skills/*` (Codex development skills).
They direct the client to inspect evidence and ambiguity; they do not invoke an LLM
inside the server or guarantee that Codex exposes every prompt via a slash command.

## CLI target surface
- `codeatlas serve --root <absolute-path> [--watch] [--memory-write]`: MCP stdio, optional owner-only watching and trusted memory mutation, no stdout diagnostics.
- `codeatlas index --root <path> [--full]`: explicit bounded local indexing.
- `codeatlas status --root <path> [--json]`: inspect index status.
- `codeatlas doctor --root <path> [--json]`: versions, permissions, lock mode, health.
- `codeatlas integrate codex --root <path> --dry-run`: show a surgical config diff.
- `codeatlas integrate codex --root <path> --apply`: explicit backed-up config write.
- `codeatlas integrate codex --remove --dry-run|--apply`: remove only owned entry.
- `codeatlas config validate`: future target; not implemented in Phase 15.

The Phase 15 `integrate codex` commands are implemented with an owned comment marker,
entry-level dry-run diff, collision/malformed-config rejection, sibling backup and
same-directory atomic replacement. Other commands remain implementation targets only
where explicitly labelled. CLI JSON output is allowed on stdout only outside MCP
serve mode.

Phase 11 advertises the first thirteen rows above: status, asynchronous application
index jobs and all Phase 10 retrieval operations. Every handler has a generated,
closed input schema, a common generated output-envelope schema and accurate
read-only/mutation annotations. The root is never a tool argument. Application
failures use stable codes in structured `isError` results while rmcp retains
malformed-message, unknown-method and version errors.

`index_repository` durably prepares or deduplicates a job before returning, then
runs the existing engine pipeline on a retained background worker. Same-key active
requests return the existing job; competing writers receive retryable `WRITER_BUSY`.
EOF cancels and joins retained workers. The server does not advertise a Tasks
capability.

Retrieval first applies the Phase 10 structural budget with transport reserve, then
the MCP adapter serializes the complete `CallToolResult` and rejects any oversized
result before rmcp frames it. Independent modern and legacy subprocess tests assert
actual emitted tool-response frames stay within 65,536 bytes. Memory tools,
resources and server prompt templates remain unadvertised until Phase 13.

Phase 12 adds no tool or MCP capability. `serve --watch` is a trusted startup option
and remains false by default. Before an explicit index call,
`repository_status.data.watch` reports enabled but not running and performs no
storage or traversal work. Once this process owns the writer it reports the concrete
backend, running/pending/reconciling state, bounded queue capacity, event/coalescing/
overflow counters, reconciliation counts, last trigger/error and reconciliation
time. It labels filesystem event delivery as degraded and names periodic scan/content-
hash reconciliation as the consistency boundary. Followers never start a watcher;
mutations remain retryable `WRITER_BUSY` while another process owns the database.

Phase 13 implements all sixteen tool definitions. Without trusted
`--memory-write`, tools/list truthfully exposes the thirteen Phase 11 tools plus
read-only `search_memories`; with it, `upsert_memory` and destructive
`forget_memory` are also exposed and backend-authorized. Memory results identify
their provenance as explicitly authored untrusted project notes and set
`authoritative_code_fact=false`. Search defaults to excluding stale evidence and
truncates only whole records under the response cap.

resources/list exposes status and map; resources/templates/list exposes typed symbol
and memory URI templates. A strict URI parser rejects other schemes, percent-encoded
or path-like IDs and traversal. Resource bodies are JSON application envelopes from
the same backend operations as their tool equivalents. Both supported lifecycle
eras exercise list/read and preserve their version-correct result fields.

prompts/list and prompts/get expose `explain_symbol`, `plan_change` and
`investigate_failure`. The server returns static prompt messages containing quoted
arguments plus instructions to distinguish evidence, candidates, stale notes and
uncertainty and to obey client approval policy. It performs no sampling/model call
and makes no claim that a particular client presents these as slash commands.
