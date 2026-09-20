---
name: ca-index-storage
description: "Implement SQLite migrations, FTS maintenance, atomic index generations, writer ownership, jobs or file watching for CodeAtlas; use for persistence and incremental consistency work."
---

# Index Storage


## Workflow
Read docs/DATA_MODEL.md and DEPENDENCY_POLICY.md. Probe the actual bundled SQLite
version/FTS5 and enforce the known WAL fix floor. Keep DB files local and owner-owned.
Keep one write-owner process, one writer thread and bounded mutations; followers
query or return retryable WRITER_BUSY. Never acquire a blocking writer lock before
MCP discovery. Serialize checkpoints and migrations through the owner.

Use immutable file versions, generation membership and atomic activation. Persist
facts from the bytes that were hashed. Every query joins the pinned generation.
Cancelled/failed/traversal-incomplete jobs retain the prior active view. A per-file
parse failure is explicit coverage loss, not reuse of stale symbols as fresh facts.

Incremental parsing means changed files are reparsed; Tree-sitter subtree reuse is
optional and requires precise Tree::edit mapping. Never hand an unchanged old tree
new text without edits. Full graph re-resolution is an acceptable first correctness
baseline. Never update only outgoing edges while leaving stale incoming bindings.

Watcher events are hints. Debounce, re-stat/re-hash, bound queues, and reconcile
missed/overflowed events. Branch changes and ignore/config/grammar changes invalidate
the right scope. Full rebuild and incremental update must converge to equivalent facts.

## Tests
Migrate old DBs, keep notes, owner/follower contention, cancelled/killed writer,
FTS deletes, rename/removal, same mtime different content, incomplete traversal,
active-reader consistency, stale cursor and WAL-safe backup. Run native OS tests.
Do not auto-delete corruption or consider copying an open .db a complete backup.
