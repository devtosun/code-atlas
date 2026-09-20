# ADR-0012 — Explicit memory, typed resources and static prompts

Status: accepted
Date: 2026-09-20

## Context

CodeAtlas already stores syntax observations and derived graph facts. Phase 13 adds
user-supplied project notes without allowing them to masquerade as extracted facts,
survive only as long as an index generation, or enable privileged writes through
repository-controlled configuration. MCP resources must not become a second path
around root, snapshot, hash or response limits. Server prompts must remain client
templates rather than an embedded model or an instruction-execution mechanism.

## Decision

Use schema v5 memory rows independent of generations. Every note records a typed
kind, text, optimistic revision, author, origin, scope and timestamps. Evidence is a
bounded exact path/content-hash pair with an optional immutable symbol observation
ID. It is validated against the active generation on write and recomputed as
verified, unverified or stale on read. Name-based rebinding is forbidden.

Expose read-only `search_memories` by default. Expose and authorize
`upsert_memory` and destructive `forget_memory` only when the trusted process starts
with `--memory-write`. Repository files cannot select this flag. Storage applies
count, byte, metadata and evidence limits and serializes writes on the existing
owner thread with exact revision checks.

Expose exact status/map resources and symbol/memory URI templates under the
`codeatlas://repo/` scheme. Parse URI components with a strict allowlist and delegate
reads to the same backend operations used by tools. Reject filesystem schemes,
encoding, separators and traversal. Apply the same complete response cap.

Expose `explain_symbol`, `plan_change` and `investigate_failure` as static rmcp prompt
templates. Quote caller arguments, label source and memory text as untrusted data,
request evidence and uncertainty, and defer mutation/external-action approvals to
the client. CodeAtlas does not invoke an LLM and does not assume client UI exposure.

## Consequences

- Reindex, failed generations and generation GC cannot erase explicit notes.
- A changed/deleted file or missing exact symbol makes evidence stale; it does not
  silently attach to a same-name declaration.
- Followers can search notes but mutation receives the existing retryable writer
  contention behavior.
- Local SQLite plaintext may contain note text, and remote MCP clients may send it
  to a remote model under their own policy.
- Static prompt wording reduces ambiguity but cannot guarantee a remote model will
  resist every prompt injection.

## Evidence

Schema migration/storage tests cover revision conflicts, evidence validation,
staleness, FTS, reindex/GC preservation and exact deletion. Modern and legacy real
stdio clients cover default-hidden mutation tools, all sixteen opted-in tools,
resources/templates/read, prompts/list/get, restart retention, URI attacks,
oversized notes and synthetic prompt-injection text.
