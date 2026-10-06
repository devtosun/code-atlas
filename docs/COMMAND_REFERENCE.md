# CodeAtlas command reference

All repository, executable and configuration arguments used for integration must be
absolute paths. `serve` reserves stdout for MCP frames; diagnostics use stderr.

## Runtime and indexing

```text
codeatlas serve --root ABSOLUTE_PATH [--watch] [--watch-poll] [--memory-write]
codeatlas doctor [--root ABSOLUTE_PATH] [--json]
codeatlas index --root ABSOLUTE_PATH [--full] [--request-key KEY] [--json]
codeatlas status --root ABSOLUTE_PATH [--json]
```

`serve` starts one client-owned stdio process. It does not require or create a
daemon, network listener, external database, language SDK, package restore or model
download. Indexing is explicit; MCP discovery and `tools/list` do not wait for it.

## Codex integration

```text
codeatlas integrate codex --root ABSOLUTE_PATH [--binary ABSOLUTE_PATH] \
  [--config ABSOLUTE_PATH] --dry-run
codeatlas integrate codex --root ABSOLUTE_PATH [--binary ABSOLUTE_PATH] \
  [--config ABSOLUTE_PATH] --apply
codeatlas integrate codex [--config ABSOLUTE_PATH] --remove --dry-run
codeatlas integrate codex [--config ABSOLUTE_PATH] --remove --apply
```

Without `--binary`, the running executable's canonical absolute path is used. Without
`--config`, the target is `$CODEX_HOME/config.toml` when `CODEX_HOME` is set, otherwise
`$HOME/.codex/config.toml`. Dry-run never creates a directory or file and prints only
the owned entry-level diff, not unrelated configuration values.

Apply rejects relative/non-executable binary paths, malformed TOML, a symlinked config
file, and an existing unowned `mcp_servers.codeatlas` entry. It preserves unrelated
TOML with `toml_edit`, writes a timestamped sibling backup, then uses a same-directory
atomic replacement. The generated entry uses `required = false`, the normal 10-second
startup timeout and 60-second tool timeout.

Removal accepts only the table carrying CodeAtlas's ownership marker. It leaves other
MCP servers, settings, backups, source, indexes and project memories untouched. Data
purge is intentionally not an integration command.

## Native package workflow

On a native macOS ARM64 host with the pinned toolchain and cached locked dependencies:

```text
python3 scripts/package_macos.py
python3 scripts/phase15_package_smoke.py \
  dist/codeatlas-0.1.0-aarch64-apple-darwin.tar.gz
```

The workflow builds with `cargo build --release --locked --offline`, checks the native
Mach-O architecture/system libraries, embeds notices and quick-start material, writes
inner and outer SHA-256 checksums, and produces a deterministic tar/gzip stream.
`--skip-build` packages the already-built release binary. Linux and Windows package
commands do not exist until their deferred native validation passes.
