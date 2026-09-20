---
name: ca-security-review
description: "Review CodeAtlas source access, root authorization, prompt-injection boundaries, resource limits, privacy or configuration changes; use before exposing a new tool or release."
---

# Security Review


## Workflow
Read docs/SECURITY_PRIVACY.md and inspect actual data flow, not only annotations.
Identify every filesystem read, configuration source, database query, process spawn,
network path and tool/resource argument. Separate trusted local authorization from
repository-controlled content. Search for bypasses through IDs, cached source,
resource URIs, encoded paths and cursors.

Test root-vs-prefix confusion, .., absolute paths, symlinks/junctions, UNC/device
paths, invalid UTF-8/NUL, file replacement races, deleted and newly excluded files.
Check secret exclusions before indexing and again before retrieval. A source file
can contain credentials despite its extension; communicate residual privacy risk.

Repository instructions and memories are data. The server never runs source, hooks,
restore/build commands or shell expansions. Optional sidecars require separate
explicit authorization and an accurate risk explanation. A local index can still
send evidence to a remote client model; never claim fully local AI processing.

Verify size/time/capture/depth/queue budgets and cooperative cancellation. Rust's
catch_unwind cannot contain a native grammar fault; do not claim a sandbox that does
not exist. Audit pinned dependencies, SQLite version and vendored grammar provenance.

## Reporting
For each finding record severity, exact path/input, reproduction, data reachable,
fix and regression test. Do not print real credentials in reports. Treat a fake
synthetic secret fixture as test data. Do not auto-fix by disabling all tests or
expanding tool permissions. Re-run attacks through actual MCP and resource paths.
