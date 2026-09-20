---
name: ca-release-validation
description: "Validate native builds, performance evidence, package contents and safe Codex installation/removal for CodeAtlas; use for hardening or release phases, not premature publishing."
---

# Release Validation


## Workflow
Read docs/TEST_MATRIX.md and SECURITY_PRIVACY.md. Verify the phase ledger against
actual test evidence. Build the pinned release with --locked. Check that grammar
queries and assets are embedded and that launch needs no language runtime, daemon,
Docker or model download. Record real dynamic dependencies for each target.

Test the extracted package, not only target/release inside a developer worktree.
Run modern/legacy stdio smoke, no-index startup, corrupt/busy DB, EOF cleanup and
source paths with spaces/Unicode. Native execution is necessary before claiming
macOS/Windows/Linux support. Cross-compilation alone is insufficient.

Measure startup, lookup, incremental changes, RSS and index throughput on a recorded
baseline. Targets are not facts; publish failures and limitations. Verify full vs
incremental corpus equivalence and bounded result output after packaging.

Codex installation uses an absolute compiled binary path, never cargo run at startup.
Show a config diff; preserve unrelated TOML/settings/comments/MCP entries; back up
before apply; required=false. Removal deletes only the owned entry and does not
purge persistent notes by default. Test with unrelated entries and malformed config.
Never modify PATH/system policy or pipe a remote installer into a shell silently.

Generate checksums and dependency notices. Signing/publishing/uploading is a separate
user-authorized action; do not claim signed/notarized artifacts without evidence.
Produce a final release-readiness report with exact unexecuted checks and next steps.
