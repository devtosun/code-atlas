# ADR-0010: typed MCP tools and detached application jobs

Status: accepted
Date: 2026-09-19
Phase: 11

## Context and evidence

Phase 10 implemented repository-bound retrieval use cases but exposed only
repository_status. Phase 11 needs thirteen real tools in both verified protocol eras
without moving SQL, traversal or ranking policy into rmcp handlers. Indexing can
outlive one tool call, while discovery, status, job reads and cancellation must stay
responsive. Application jobs must not be misrepresented as standardized MCP Tasks.

Subprocess tests exercise the full binary with modern 2026-07-28 discovery metadata
and legacy 2025-11-25 initialization, all thirteen tools, generated schemas,
malformed inputs, writer contention, active-job idempotency, cancellation races,
response caps, stdout purity and EOF shutdown.

## Decision

ca-mcp owns closed serde/schemars request DTOs, tool annotations, the common
application envelope and application-error-to-isError translation. Its handlers
delegate one typed BackendRequest through a ToolBackend port and always run the
blocking backend on Tokio's blocking executor. rmcp remains responsible for JSON-RPC,
stdio framing, protocol negotiation and protocol errors.

The CLI composition implements the backend by adapting the existing engine indexing
and retrieval use cases to SQLite, languages and the startup-authorized root. No tool
accepts a root path. Enum, range, ID, path, cursor, freshness and response limits are
validated before or within the owning use case.

Indexing is split into a bounded prepare step and run_prepared execution. The MCP
backend durably creates or deduplicates a queued application job, returns its ID,
then runs the existing index pipeline on a named background thread. The backend
retains the owner's storage handle and cooperative cancellation token for each active
job. Same-key active requests return that job; different mutations receive retryable
WRITER_BUSY. EOF cancels and joins every retained worker before server exit.

Every success and application failure has structured content plus a JSON text
fallback. Retrieval budgets reserve transport-envelope space, and the final
CallToolResult is serialized and checked before rmcp adds the small JSON-RPC frame.
Oversized results become a bounded RESPONSE_TOO_LARGE error; JSON is never cut.

## Alternatives considered

- Run indexing inside the tool future. Rejected because repository work would hold
  the request and compromise transport responsiveness.
- Advertise MCP Tasks. Rejected because these application jobs do not implement or
  negotiate the standardized Tasks extension.
- Put SQLite and retrieval policy in ca-mcp. Rejected because transport would
  duplicate application rules and violate the dependency boundary.
- Spawn untracked workers. Rejected because EOF and cancellation would leave
  ambiguous durable state and unbounded retained handles.
- Return protocol errors for all failures. Rejected because actionable application
  failures belong in visible isError results.

## Trade-offs and failure modes

Storage open and job creation happen before index_repository can return a durable ID,
but traversal, parsing, resolution and activation happen afterward. Only one writer
owns a database; another mutation receives a retry hint rather than a hidden queue.
Completed handles are reaped on later index calls and all remaining handles are
joined at shutdown.

The output schema describes a stable envelope whose data member is tool-specific
JSON. Per-tool semantic output is validated by end-to-end assertions; later versions
can add narrower data schemas without changing the envelope.

## Security and compatibility impact

The model cannot supply or widen the authorized root. Job and generation IDs resolve
only in the root's database; cursors remain repository-bound. Source paths use the
existing normalized, ignore-aware, no-follow reader and freshness checks. Tools
execute no repository code, restore, hook, shell command or network request.

Official rmcp 3.4.0 provides both wire eras. Only index_repository and cancel_job are
marked mutating; every tool is non-destructive and closed-world. No Tasks capability
is enabled.

## Tests and rollback strategy

Unit tests validate all thirteen generated input/output schemas and annotations.
Independent subprocess tests drive every tool through both wire eras with persisted
fixture data, exercise errors, locks, cancellation and EOF, and assert stdout JSON
purity plus the 65,536-byte tool response cap.

Rollback removes the public tool adapter and restores status-only advertisement.
Schema version 4, active generations and user-owned databases remain readable; a
rollback must not delete databases or memories.

## References

- prompts/11-mcp-tools.md
- docs/ARCHITECTURE.md
- docs/MCP_CONTRACT.md
- docs/SECURITY_PRIVACY.md
- docs/reports/11-mcp-tools.md
