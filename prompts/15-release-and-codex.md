# Phase 15 — Native packaging and safe Codex integration

$ca-phase-driver $ca-release-validation $ca-mcp-contract

## Task and scope
Produce locally testable native release packages and safe installation/removal workflows; do not publish or alter the user machine without explicit authorization.

## Prerequisites
Phase 14 acceptance gates must be complete; verify its report first.

## Read before editing
- `AGENTS.md`
- `docs/PROJECT_STATE.md`
- `docs/TEST_MATRIX.md`
- `docs/SECURITY_PRIVACY.md`
- `config/codex.macos.example.toml`
- `config/codex.windows.example.toml`

## Implementation steps
1. Create reproducible --locked native release workflows for primary supported
   targets. Verify actual runner availability and target execution before promising
   macOS ARM64, Windows x64 MSVC or Linux x64 support. Extra targets are separately gated.
2. Embed grammar queries and required assets; package the executable, notices,
   quick-start and checksums. Identify system library dependencies honestly. Test
   extracted archives in clean paths with spaces/Unicode and no language SDK installed.
3. Implement `integrate codex --dry-run` using toml_edit and accurate absolute binary/
   root paths. Never configure cargo run/npx/download-on-start. Preserve unrelated
   comments/settings/MCP definitions and show a surgical diff.
4. Implement explicit --apply with backup and safe atomic replacement appropriate
   to platform. Set required=false. Do not raise timeout to hide slow indexing at
   startup. Do not change PATH/global ExecutionPolicy or request admin by default.
5. Implement owned-entry removal with dry-run/apply. Preserve source and notes;
   purging data is a separate explicit destructive action. Handle malformed config
   and name collisions without replacing unrelated entries.
6. Run real installed-Codex smoke when authorized: inspect version/config, start a
   fresh session, discover tools, index fixture root, search/reference/context,
   restart, concurrent client/follower, and uninstall/remove cleanly. Without Codex
   access record this as not run and provide the exact safe manual recipe.
7. Produce release readiness report, command reference, limitations, support matrix,
   rollback steps and checksums. Signing/notarization/publishing are explicit separate
   operations; do not claim success without credentials and actual execution.

## Acceptance gates
- Packaged binaries pass modern/legacy stdio and fixture indexing tests after extraction.
- Launch requires no mandatory daemon, DB service, model download or language SDK.
- Existing Codex config round-trips with unrelated entries unchanged; required=false.
- Busy/corrupt index cannot block a healthy Codex session through required startup.
- Removal changes only owned integration data and preserves notes/source.
- Release report clearly separates built, executed, client-tested and untested targets.

## Out of scope
No public release upload, automatic privileged installation, signing claims or optional framework/LSP features.

## Execution and handoff contract
Implement this phase, not only a plan. Preserve unrelated changes. Do not run the
next phase automatically. Use current pinned documentation/source, not guessed API
names. Record blocked tooling or unavailable native targets honestly. Never weaken
assertions, fabricate benchmarks, or advertise an unimplemented capability to pass.
Run the focused tests plus applicable workspace gates. Write
`docs/reports/15-release-and-codex.md` using the phase-report template; update project state,
test matrix and relevant ADRs. End with changes, exact commands/results, limitations
and the next phase filename. Do not commit/push/publish or alter user config without
authorization. If a gate fails, fix it within this phase or mark the phase blocked.
