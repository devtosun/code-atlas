# Phase 13 — Explicit project memory, resources and prompt templates

$ca-phase-driver $ca-mcp-contract $ca-index-storage $ca-security-review

## Task and scope
Add optional source-grounded project notes and MCP resources/prompt templates, distinct from automatically extracted code facts.

## Prerequisites
Phase 12 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/MCP_CONTRACT.md`
- `docs/DATA_MODEL.md`
- `docs/SECURITY_PRIVACY.md`

## Implementation steps
1. Implement note kinds decision/convention/pitfall/task with text, revision,
   author/origin metadata, scope, timestamps and optional evidence refs/hashes.
   Notes are explicitly supplied; the server does not invent architectural decisions.
2. Add search_memories, opt-in upsert_memory and opt-in forget_memory. Enforce
   size/count limits, optimistic concurrency and correct mutation/destructive hints.
   Trusted runtime policy controls writes; repository config cannot enable them.
3. Validate referenced symbols/file hashes and mark notes stale/unverified when
   evidence changes or is deleted. Do not silently rebind a renamed symbol by name.
   Search distinguishes notes from observed facts and defaults to excluding stale.
4. Keep persistent notes independent of index rebuild/GC. Migration, backup,
   uninstall and repair operations must not silently discard them.
5. Add typed resources from MCP_CONTRACT.md; reuse tools' root/hash/snapshot/budget
   policy. Add explain_symbol, plan_change and investigate_failure prompt templates
   that instruct the client to use evidence/uncertainty and obey its own approvals.
6. Treat note text and repository excerpts as untrusted data with clear provenance.
   Test prompt-injection text as synthetic content; do not execute it or claim the
   server can fully control a remote model's behavior.
7. Test both lifecycle eras, memory revision conflicts, stale evidence, note deletion,
   no-write policy, resource URI escapes, output schemas and index rebuild retention.

## Acceptance gates
- All sixteen tools are implemented with truthful enabled/disabled capabilities.
- Notes cannot become authoritative code facts merely by being stored.
- Writes/deletes require trusted opt-in and obey revision/size/policy checks.
- Reindex/GC/restart/migration preserve notes; stale evidence is surfaced.
- Resources cannot bypass source authorization or stale-hash protection.
- MCP prompt templates work without an LLM inside the server; client UI exposure is not assumed.

## Out of scope
No automatic chat-history scraping, secret collection, synthesized facts, global user memory import or mandatory memory writes.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/13-memory-resources-prompts.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
