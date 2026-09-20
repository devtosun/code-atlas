# Phase 17 — Optional explicitly approved LSP enrichment

$ca-phase-driver $ca-rust-architecture $ca-graph-resolution $ca-security-review

## Task and scope
Add an opt-in semantic-enrichment boundary while preserving complete daemon-free Tree-sitter-only operation. Implement one proven provider before promising more.

## Prerequisites
Phases 00–15 must be complete. This is OPTIONAL and requires an explicit request.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/ARCHITECTURE.md`
- `docs/LANGUAGE_SUPPORT.md`
- `docs/SECURITY_PRIVACY.md`
- `docs/DEPENDENCY_POLICY.md`

## Implementation steps
1. Write an ADR explaining process, privacy and build-tool risks. LSP servers can
   invoke toolchains/build tasks: explicit user consent and trusted executable/config
   allowlists are required. Repository config cannot enable or substitute a server.
2. Inspect current official language-server and LSP documentation. Choose and pin
   a first provider (Rust or Go is a practical initial target); verify its settings
   for offline use, dependency operations and build-script execution. Do not assume
   every server has a safe no-execution mode or auto-install it.
3. Implement an optional semantic provider port and bounded child-process supervisor:
   startup/deadline limits, cancellation, exit cleanup, no orphan processes and a
   circuit breaker. Sidecar failure never blocks MCP discovery or lexical tools.
4. Implement negotiated position encoding and exact UTF-8/UTF-16 mapping. Accept
   semantic results only for the same root, source version/hash and configuration.
   Reject stale responses. Persist provider/version/evidence separately from syntax facts.
5. Add semantically_resolved static declaration/reference edges without erasing
   lexical/candidate provenance. Type-level resolution still does not prove dynamic
   runtime dispatch or every execution path. Default operation remains no sidecars.
6. Test missing/crashing/slow provider, source changes in flight, encoding edge cases,
   denied executable/config, process cleanup and deterministic fallback. Document
   other languages as provider-not-implemented until each is actually integrated.

## Acceptance gates
- Default build/runtime remains fully useful for all seven Tree-sitter languages.
- The first selected provider has real opt-in, version-checked end-to-end tests.
- No unauthorized process launch, automatic download or repository-config privilege escalation.
- Stale semantic results cannot attach to a newer generation/source hash.
- Sidecar failures fall back without blocking startup or corrupting syntax facts.
- Documentation distinguishes implemented providers from future extension candidates.

## Out of scope
No mandatory LSP runtime, automatic dependency restore, all-language semantic claim or embedded cloud AI.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/17-semantic-enrichment.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
