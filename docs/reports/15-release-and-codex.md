# Phase 15 report — native packaging and safe Codex integration

State: completed for the supported macOS ARM64 target, with model-backed Codex smoke not run
Date: 2026-09-20
Git revision and local diff: `8461837e5cff`; Phase 15 changes are uncommitted and unpushed
Host / target / toolchain: macOS 27.0 (26A428) arm64 / `aarch64-apple-darwin` /
rustc 1.98.1 (48a229cea), Cargo 1.98.1, Apple clang 17.0.0

## Scope implemented

- Added an exact locked `toml_edit` dependency and `codeatlas integrate codex` with
  dry-run/apply/remove, canonical absolute executable/root paths, comment-based entry
  ownership, collision/malformed/symlink rejection, backup and same-directory atomic
  replacement. The generated server remains optional with `required = false`.
- Added a native macOS ARM64 packaging workflow. It builds release/locked/offline,
  validates the real Mach-O architecture and dynamic libraries, generates locked
  dependency notices, packages quick-start/security/build metadata and writes inner
  plus outer SHA-256 checksums in a deterministic tar/gzip stream.
- Added an extracted-package smoke that runs from a Unicode/space path with an empty
  `PATH`, indexes seven language fixtures, exercises modern and legacy MCP, concurrent
  owner/follower reads, corrupt-database startup discovery, EOF and disposable config
  install/remove. No language compiler, SDK or package manager is visible.
- Added a dispatch-only GitHub Actions macOS ARM64 package workflow with an explicit
  `Darwin-arm64` runtime assertion. GitHub's current hosted-runner reference lists
  `macos-15` as an ARM64 label; the workflow itself has not yet been dispatched.
- Added command, quick-start, rollback, privacy and manual installed-Codex guidance,
  plus ADR-0013 and refreshed supply-chain evidence.

## Files changed

- Runtime: `Cargo.toml`, `Cargo.lock`, `crates/ca-cli/Cargo.toml`,
  `crates/ca-cli/src/main.rs`, `crates/ca-cli/src/codex_integration.rs`
- Packaging/tests: `scripts/package_macos.py`, `scripts/phase15_package_smoke.py`,
  `.github/workflows/release-macos-arm64.yml`, `.gitignore`
- User material: `docs/release/QUICKSTART.md`, `docs/COMMAND_REFERENCE.md`,
  `docs/manual/15-codex-smoke.md`, `LICENSE-NOTICE.md`
- Policy/state: `docs/adr/0013-native-package-and-owned-codex-config.md`,
  `docs/SECURITY_PRIVACY.md`, `docs/DEPENDENCY_POLICY.md`, `docs/MCP_CONTRACT.md`,
  `docs/PROJECT_STATE.md`, `docs/TEST_MATRIX.md`, `README.md`, `README.tr.md`,
  `config/dependency-lock.json`
- Evidence: `docs/reports/artifacts/15-package-smoke.json`,
  `docs/reports/artifacts/15-dependency-inventory.json`

## Decisions and ADRs

ADR-0013 records deterministic native packaging and comment-marked ownership. A
comment is used instead of an unknown configuration key so Codex strict parsing is
not burdened by tool metadata. An unmarked `mcp_servers.codeatlas` entry is never
replaced or removed. Dry-run prints only the affected table to avoid disclosing other
configuration values. Backup and temporary files begin with restrictive permissions,
then adopt the original mode.

The package remains unsigned and unnotarized. Deterministic bytes and checksums detect
accidental/tampered changes after the digest is obtained through a trusted channel;
they are not a signature. Linux and Windows packaging is intentionally absent until
native qualification executes.

## Package identity and dependencies

| Item | Value |
|---|---|
| Native executable | `codeatlas` 0.1.0, stripped Mach-O arm64, 25,013,888 bytes |
| Executable SHA-256 | `45f11cecbadafc64456fd06ea36dcca9e140737d808f42ac56878afee6227b82` |
| Archive | `codeatlas-0.1.0-aarch64-apple-darwin.tar.gz`, 5,484,074 bytes |
| Archive SHA-256 | `57e64086ba5fb066f0fd79f1a2aeda71080e7cb68e8a7c8a7ca023327485c427` |
| Dynamic libraries | CoreFoundation, CoreServices, `/usr/lib/libiconv.2.dylib`, `/usr/lib/libSystem.B.dylib` |
| Embedded assets | all Tree-sitter query text through `include_str!`; statically linked grammar parsers; bundled SQLite/FTS5 |
| Runtime downloads/services | none; no daemon, port, DB service, model download or language SDK |

The refreshed native metadata contains 246 third-party registry packages. Cargo-audit
scanned 299 locked dependencies against 1,251 cached advisories with zero findings;
cargo-deny passed advisories, bans, licenses and source policy. Generated notices in
the archive list each locked third-party package/version/license/upstream.

## Commands and evidence

| Command | Exit code | Result | Log / artifact |
|---|---:|---|---|
| `cargo test -p ca-cli codex_integration --locked --offline` | 0 | 4 config ownership/round-trip tests pass | test output |
| `cargo run -p xtask --locked --offline -- verify` | 0 | fmt, strict workspace clippy, 116 tests pass; 2 helper tests ignored; all language/real-corpus gates pass | command output |
| `python3 scripts/phase14_dependency_audit.py --output docs/reports/artifacts/15-dependency-inventory.json` | 0 | locked metadata, audit and deny pass | `15-dependency-inventory.json` |
| `python3 scripts/package_macos.py` | 0 | native locked/offline release archive produced | `dist/` (ignored local output) |
| `python3 scripts/package_macos.py --skip-build` | 0 | second package byte-identical at the same SHA-256 | command output |
| `python3 scripts/phase15_package_smoke.py … --codex /opt/homebrew/bin/codex` | 0 | extraction/index/protocol/follower/corrupt/EOF/config checks pass | `15-package-smoke.json` |
| `otool -L target/release/codeatlas` | 0 | only recorded Apple system libraries | command output / package metadata |
| `codex --version`; disposable `codex mcp list` | 0 / 0 | `codex-cli 0.154.0`; generated entry parses | package smoke artifact |
| `python3 scripts/validate_kit.py` | 0 | 85 Markdown files, prompts/skills/TOML/fixtures/state validate | command output |
| `shasum -a 256 -c …sha256` from repository root | 1 | diagnostic invocation used the wrong working directory; archive was not opened | command output |
| same `shasum` from `dist/` | 0 | outer archive checksum is `OK` | command output |

## Acceptance criteria

| Criterion | Passed / failed / not run | Evidence |
|---|---|---|
| Extracted modern/legacy MCP and fixture indexing | passed | 14 tools in both eras; 7/7 files parsed, zero failed |
| No mandatory daemon/service/model/SDK | passed | empty-PATH extracted smoke; local stdio only |
| Embedded grammars/queries/notices/checksums | passed | registry gate, archive listing, inner/outer SHA-256 |
| Existing Codex config unrelated content unchanged | passed | unit and package round-trip; `required = false` |
| Malformed config/name collision fail closed | passed | focused unit tests leave bytes unchanged |
| Busy/corrupt index does not block healthy Codex startup | passed | follower status works; corrupt DB still completes discovery/tools-list |
| Owned removal preserves source/notes/other MCP entries | passed | source and notes sentinels plus disposable config round-trip |
| Installed Codex client configuration parse | passed | `codex-cli 0.154.0` against disposable `CODEX_HOME` |
| Installed Codex model-backed tool session | not run | fixture text transmission was not explicitly authorized; safe recipe supplied |
| macOS ARM64 native build/execution | passed | local native host and extracted package |
| macOS ARM64 hosted workflow execution | not run | workflow configured; dispatch has not executed |
| Linux x64 / Windows x64 MSVC | not run / unsupported | explicitly deferred |

## Support matrix

| Target | Built | Native executed | Extracted package | Installed client | Supported |
|---|---:|---:|---:|---:|---:|
| macOS ARM64 | yes | yes | yes | config parse yes; model session no | yes, unsigned local package |
| Linux x64 | no | no | no | no | no |
| Windows x64 MSVC | no | no | no | no | no |

## Rollback

Run `integrate codex --config ABSOLUTE_PATH --remove --dry-run`, review the owned
table diff, then repeat with `--remove --apply`. This removes no index, source or
project memory. Keep the timestamped sibling backup until Codex is verified. If
necessary, restore that backup manually while Codex is stopped. Public publishing,
system installation, PATH changes, signing and notarization were not performed, so
there is no corresponding remote/system rollback.

## Known limitations and risks

- The real user `~/.codex/config.toml` was neither read nor changed. The installed
  client check used a disposable config, and no model-backed Codex turn ran. The exact
  user-authorized recipe is `docs/manual/15-codex-smoke.md`.
- The artifact is unsigned/unnotarized and local-only. Gatekeeper behavior and a
  trusted distribution channel are not qualified; no public release claim is made.
- GitHub `macos-15` runner availability was checked against current official docs and
  asserted in the workflow, but this repository's release workflow was not dispatched.
- Linux/Windows archive formats, dynamic libraries, atomic replacement semantics,
  permissions, junctions and native client behavior remain unexecuted.
- Existing Phase 14 limits remain: syntax evidence is not compiler semantics, native
  grammar code is trusted, the source-read TOCTOU residual remains, and this repository
  itself has one source file above the configured per-file fact limit.
- A remote Codex/model can receive source and memory returned by MCP. Local indexing
  is not an end-to-end local inference guarantee.

## Blockers and safe next actions

No blocker remains for the scoped unsigned macOS ARM64 local package. A complete
installed-Codex model session requires explicit authorization to transmit the chosen
non-private fixture. Signing/notarization needs separate credentials and authorization.
Linux and Windows require their native qualification prompt before package support.

Current OpenAI documentation confirms the stdio table fields and default 10/60-second
timeouts, `required` behavior and `codex mcp list`; it also documents ephemeral
non-interactive sessions and read-only sandboxing:
https://learn.chatgpt.com/docs/extend/mcp?surface=cli and
https://learn.chatgpt.com/docs/non-interactive-mode. GitHub's hosted runner reference
lists `macos-15` as ARM64:
https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job.

## Exact next prompt

`prompts/14-linux-windows-native-validation.md`
