# Dependency and protocol verification policy

Phase 00 produced a compatible, compiled lockfile for the isolated spike rather
than inventing one. Exact results are in `spikes/compatibility/Cargo.lock` and
`config/dependency-lock.json`. Research and execution date: 2026-09-19; recheck
when upgrading or moving the pins into the application workspace.

Phase 01 created the production `Cargo.lock` and pinned Rust 1.98.1 plus the exact
dependencies first used by the status-only application: rmcp 3.4.0 with `macros`, `server`
and `transport-io`; serde 1.0.229; serde_json 1.0.151; schemars 1.2.2; thiserror
2.0.20; Tokio 1.53.1; clap 4.6.7; tracing 0.1.44; and tracing-subscriber 0.3.23.

Phase 02 promoted exact production pins for rusqlite 0.40.2 with `bundled-full`,
gix 0.87.1 with default features disabled and only `sha1`, ignore 0.4.33, fs4 1.1.0,
BLAKE3 1.8.7 and libc 0.2.189 on Unix. The linked production SQLite reports 3.53.2,
FTS5 and WAL on macOS arm64, satisfying the 3.51.3 policy floor. Tree-sitter grammar
dependencies remain confined to the Phase 00 spike until Phase 03.

Phase 03 promoted the exact Tree-sitter runtime and grammar pins into production.
Phases 04–11 add no new third-party runtime dependencies. Phase 09 reuses the pinned
`ignore`, BLAKE3, Tree-sitter and rusqlite boundaries through engine ports and CLI
composition. Root `go.mod` and `pubspec.yaml` files are parsed by bounded in-process
data rules; no Go/Dart SDK, package manager, compiler or new manifest package is
introduced. Phase 10 adds existing workspace `serde`/`serde_json` dependencies to the
engine crate for exact response serialization; it resolves no new package. Phase 11
declares the already pinned serde_json and Tokio packages directly in ca-mcp for
structured errors, byte accounting and blocking-executor dispatch. The production
lock remains committed and `--locked` gates pass.

Phase 12 promotes exact notify 8.2.0 into the CLI composition crate with its default
macOS FSEvents support and platform backends from the locked dependency graph. The
crate is used only after an explicit watch opt-in and writer ownership; no dependency
is downloaded or loaded at runtime. PollWatcher is the portable fallback and enables
content comparison because event metadata alone is not a consistency boundary.

Phase 13 adds no third-party dependency. Schema-v5 memory transactions use the
existing rusqlite/SQLite/FTS5 boundary, typed resources and prompts use the pinned
rmcp 3.4.0 SDK, and serialization continues through the pinned serde/schemars pair.

Phase 14 adds no production third-party dependency. Schema v6 uses the existing
SQLite boundary to add `generation_files(file_version_id)` and
`symbols(lower(spelling))` indexes. The linked release binary still reports SQLite
3.53.2 with FTS5 and WAL, above the 3.51.3 floor. Three fuzz targets are isolated in
the workspace-excluded `fuzz/` package with exact `libfuzzer-sys = 0.4.10`; this is a
development-only declaration and is not linked into the production binary.

The macOS-target `cargo metadata --locked --offline` inventory contains 241
third-party registry packages, no Git/path third-party source and no missing declared
license/license-file metadata. `deny.toml` explicitly permits only the reviewed
license set and crates.io registry, rejects wildcard and unknown sources, rejects
yanked packages and reports duplicate versions without hiding them. Cargo-audit
0.22.2 scanned 294 locked dependencies against 1,251 cached RustSec advisories with
zero findings; cargo-deny 0.20.2 passed advisories, bans, licenses and sources. The
isolated nightly/cargo-fuzz toolchain built and ran all three targets; it is not part
of the production toolchain or runtime. See the dependency and fuzz artifacts under
`docs/reports/artifacts/`.

## Lock manifest requirements
Record Rust toolchain/MSRV/target, rmcp version and features, supported protocol
revisions, serde/schemars pairing, tree-sitter runtime ABI range, every grammar
package version/source/checksum/ABI, query hashes, SQLite runtime version and compile
options, licenses, provenance, tested OSes and exact commands. Store in
`docs/GRAMMAR_MATRIX.md` and machine-readable `config/dependency-lock.json`. Phase 00
created both. Keep Cargo.lock for binaries; pin Git dependencies to commits only.

## Tree-sitter
One chosen runtime version must accept every required grammar. Check the runtime's
supported ABI interval and each grammar ABI, then parse and execute real queries.
Cargo compilation by itself does not prove language-version coverage. TS and TSX
must both be loaded/tested. JSX has separate fixture coverage even when sharing a
JavaScript grammar. Do not use unsafe casts to bridge incompatible Language types.

For Dart, inspect the published package metadata, source and bindings rather than
assuming a similarly named GitHub repo is its source. Prefer a maintained published
binding; otherwise vendor a reviewed grammar snapshot with license, provenance,
checksum and reproducible build notes. If regenerating a parser is unavoidable,
pin generator and inputs, include scanner sources, verify all corpus tests, and
keep regeneration a build/development operation. Never download grammars at startup.

## MCP
The official versioning page currently identifies 2026-07-28, with modern discovery
and per-request metadata; 2025-11-25 uses the legacy handshake. The official SDK README
documents both. Phase 00 selected rmcp 3.4.0 and proved both in transport tests.
Do not copy old SDK tutorials or wire structs into a newer SDK without compiling.
Record the locally installed Codex version and what protocol it actually uses.

## SQLite WAL safety floor
SQLite documents a WAL-reset corruption fix in 3.51.3 and later, with backports to
3.44.6 and 3.50.7 (S16). Policy: select a current compatible bundled release with
SQLite >= 3.51.3; a backport exception needs an ADR and source-verifiable proof.
Inspect the bundled runtime version, not only the rusqlite crate version. This is a
specific known-fix floor, not a claim that every later release is bug-free. Recheck
current advisories and test concurrency/recovery. Verify FTS5 at runtime in the spike.

## Reproducibility and supply chain
No wildcard production versions, floating Git branches, unreviewed vendor files or
unbounded build downloads. Record third-party notices. Run cargo audit / cargo deny
when available; findings are reported, not blindly suppressed. Avoid remote install
script execution. Installer checksums are verified before launch; publishing/signing
requires user authorization and actual credentials. Do not fabricate signatures.
