# ADR-0014: complete analysis identity, bounded history and SDK wire guards

Status: accepted
Date: 2026-10-06
Scope: CR-001–CR-013 review repairs, not a new implementation phase

## Decision

Schema v7 renames the v6 unique-key column to `analysis_key`, preserves the old
extractor fingerprint separately and adds a per-version configuration fingerprint.
New keys length-frame all five analysis inputs. Legacy keys get a separate namespace
and an empty configuration fingerprint so the next explicit index reparses them.
Migration is transactional and preserves version IDs and all dependent rows. A
cache hit never mutates an immutable version. No database is implicitly recreated.

Retention keeps active plus its immediate predecessor and a building candidate.
Expired parent pointers are detached before superseded rows and orphan versions are
collected. Notes are independent and retained. Already-open WAL readers continue
reading their snapshot; public cursors require the active generation and are stale
after activation. This does not promise a wall-clock lease, a byte quota or bounded
WAL size with indefinitely held read transactions. Job records remain durable;
this policy bounds generation-dependent graph and file-version history.

Index workers use bounded source and result queues of the same capacity. Production
is chunked at that capacity and results are drained/persisted before the next chunk.
Factory failures occur before worker launch. On processing failure both endpoints
close before joining workers, cancellation propagates, and every handle is joined
even if a worker panics. This bounds parsed-but-unpersisted data by queue plus batch,
at the cost of a chunk barrier. It is not process isolation from native faults.

MCP framing remains the official rmcp codec and lifecycle. Use its 1 MiB ingress
limit and reject serialized request IDs over 128 bytes before SDK dispatch by ending
the connection. Prompt fields are bounded at 8192/4096 UTF-8 bytes and their generated
result is serialized again against the response budget. A final typed SDK-message
sink checks the complete JSON line including its newline against 65536 bytes. If an
SDK-generated error itself exceeds the budget, cancellation closes the connection
instead of emitting an oversized frame or leaving the client waiting indefinitely.
No protocol handshake, version wire model, or JSON framing is reimplemented.

Search uses SQL keysets before per-page limits, disjoint direct-ranking tiers and
one minimum FTS score per file-version/name. Rust keeps final deterministic ranking
and response fitting. A global first-10000 candidate window no longer decides that
a later page is complete.

## Evidence and rollback

See `docs/reports/review-fixes.md` for native tests and artifacts. Regressions include
equal-range EOF scopes, rejected full/targeted reads, receiver negatives, scoped
imports, Go directories, Dart combinators, ECMAScript exports, immutable analysis
versions, v6 migration/rollback, bounded queue high-water/panic, generation GC,
large pagination and both MCP eras. Rolling back code requires schema-v7 support;
use an explicit pre-upgrade backup, never delete user notes or source to downgrade.
