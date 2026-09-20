# ADR-0007: Bounded persistent index jobs and atomic generations

Status: accepted
Date: 2026-09-19

## Context

Phase 08 must connect safe traversal, the nine required language/dialect extractors
and SQLite without making protocol discovery wait for repository work. Incremental
correctness cannot rely on mtime or size, and an incomplete traversal cannot safely
infer deletion. Parser work is native and cooperative: dropping an async future does
not stop work already running inside Tree-sitter.

The engine must remain independent of SQLite and grammar crates. The application
also needs one write owner, query-only followers, durable job state, crash recovery
and a guarantee that a failed candidate never replaces healthy active data.

## Decision

- Put the indexing use case in `ca-engine` behind `IndexStore`,
  `ExtractionWorkerFactory` and `ExtractionWorker` ports. Compose `ca-storage` and
  `ca-languages` only in `ca-cli`.
- Complete bounded ignore-aware traversal before deletion inference. Safely reread
  every selected file, hash the bytes again and pass those owned bytes to extraction.
- Feed worker-local parsers through a bounded synchronous source queue. Cap worker,
  queue, batch and warning counts; persist batches instead of retaining ASTs or all
  source text in memory.
- Reuse an immutable file version only when content, configuration, grammar, query
  and extractor fingerprints all match the active version. Never use mtime/size as
  the correctness key.
- Store language observations, diagnostics, coverage and declaration-derived FTS
  rows by immutable file version. Phase 08 generation coverage is syntax-only and
  creates no resolved semantic edges.
- Persist the job state machine and real counters. Request keys are unique for the
  repository's index operation. Cancellation is durable, idempotent and propagated
  to parser/query callbacks while queueing and awaiting worker results.
- Stage every job in a candidate generation and atomically switch the active pointer
  only after complete traversal and committing state. Failure/cancellation abandons
  and garbage-collects the candidate; restart marks prior nonterminal jobs
  interrupted and retains the existing active generation.
- Expose synchronous engine-backed `index`, `status` and `doctor` CLI commands in
  this phase. Keep MCP status-only until the protocol tool phase; do not advertise
  placeholder index/job tools or start a daemon.

## Consequences

Small and large repositories share one deterministic bounded pipeline, and readers
see either the old or new generation rather than mixed membership. Incremental work
can reuse exact versions across process restarts while content changes with identical
mtime are reparsed. Scan and parser limitations remain visible in job coverage,
warnings and errors.

The CLI call is synchronous even though its job record is durable. Cross-file
resolution, watcher reconciliation, public search and asynchronous MCP job tools are
separate later phases. In-process cancellation remains cooperative rather than hard
isolation from native grammar faults or noncooperative code.
