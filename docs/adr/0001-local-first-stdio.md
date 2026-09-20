# ADR-0001: daemon-free local stdio baseline
Status: accepted; Phase 01 stdio and Phase 02 storage lifecycle implemented

## Context
The intended user previously experienced an MCP daemon startup timeout that blocked
Codex startup. A code index should not become a mandatory IDE/session dependency.

## Decision
Use client-launched stdio with a fast protocol path. Keep persistent data in a local
SQLite database per worktree. Open/index lazily. No detached daemon or listening port.
Codex configuration uses required=false. Concurrent clients use an explicit write-owner
and read-only followers, not a hidden socket service.

## Trade-offs
Only the current writer can perform index/note mutations; followers get retryable
errors. A watcher runs only while its owner process runs. No continuous indexing
when every client is closed. A future shared daemon needs separate justification,
health/recovery/security design and must remain optional.

## Verification
Phase 01 verifies both MCP lifecycle eras against the real binary, status before any
database exists, protocol-only stdout, EOF shutdown, explicit termination and 30
cold starts under a three-second safety deadline. Phase 02 verifies that discovery
still does not open storage, an OS lock elects one writer without blocking, followers
remain query-only with explicit retryable write errors, corrupt/future databases are
not repaired or replaced, and interrupted staging cannot replace active data.
Config-preserving integration remains for the installation phase.
