---
name: ca-rust-architecture
description: "Implement or review CodeAtlas Cargo workspace boundaries, typed core models, bounded async/blocking execution and Rust error handling; do not use for grammar-specific query design alone."
---

# Rust Architecture


## Workflow
Read docs/ARCHITECTURE.md and inspect the workspace dependency graph. Keep ca-core
independent of protocol/database/parser types. Business use cases belong in
ca-engine feature modules; ca-cli composes adapters. Do not create a generic god
repository, dependency container or utilities crate without a concrete use case.

Use typed identifiers and validated limits. Keep ownership explicit across queues:
workers return owned facts, never borrowed Tree-sitter nodes. Keep sync SQLite
connections on the owning thread, and bound CPU work before spawn_blocking.
Never hold lock guards across await. Do not run persistent writer loops in a pool
intended for short-lived blocking tasks.

Cancellation needs a token checked by workers and parser/query progress hooks.
A returned timeout or JoinHandle abort is not evidence that started blocking work
stopped. After a cancelled parse, reset/recreate the parser according to its pinned
API before reusing it for a different file.

Use thiserror for domain/infrastructure errors; add context only at application
boundaries. Return errors for malformed input and resource-limit violations. Avoid
production panics and silent fallback. Unsafe code needs a small reviewed boundary,
safety explanation, relevant tests and an ADR; do not cast incompatible grammar types.

## Checks
cargo fmt, clippy, workspace tests; compile the intended feature matrix. Test
queue saturation, cancellation, task ownership and EOF shutdown. Update architecture
and error contracts when behavior changes, not just public function signatures.
