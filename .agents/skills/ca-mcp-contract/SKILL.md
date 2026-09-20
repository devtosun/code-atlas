---
name: ca-mcp-contract
description: "Implement or review CodeAtlas rmcp tools, resources, prompts, version interoperability or stdio lifecycle; use for protocol changes and client integration tests."
---

# MCP Contracts


## Workflow
Read docs/MCP_CONTRACT.md. Use a published, pinned official rmcp version and its
version-correct examples. Modern 2026-07-28 discovery/per-request metadata and legacy
2025-11-25 initialize are different wire contracts. Use SDK support and test both;
never manually mix resultType, metadata or lifecycle rules between versions.

Only advertise capabilities and tools actually implemented. Tool schemas derive
from concrete types; validate successful and error payloads. Unknown protocol
methods/malformed protocol requests remain SDK errors. Application failures have
actionable tool errors. Tools that return structuredContent provide a compatible
TextContent representation where appropriate; count BOTH in serialization budgets.

Keep startup metadata and tools/list independent of slow filesystem/DB/lock work.
All logs go to stderr in serve mode. Indexing is a bounded application job with
job_status/cancel_job; it is not standardized MCP Tasks unless separately implemented.
Annotations describe real mutation behavior and never replace policy checks.

## Tests
Spawn the real binary with piped stdio; run modern discovery and legacy initialize,
list tools, call tools, invalid args/version, output schema validation, cancellation,
parallel status during indexing, locked/corrupt DB, Unicode, stdout contamination
and EOF shutdown. Inspect frames from an independent test client, not only calls
to handler methods. Record installed Codex version and real integration results.

Resources reuse the same authorization/hash/budget checks as tools. MCP prompt
content is a client template, not a reason for the server to call an LLM.
