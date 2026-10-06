# CodeAtlas macOS ARM64 quick start

This archive contains an unsigned, unnotarized `aarch64-apple-darwin` build. It is
not a public release. Verify the outer `.sha256` file before extraction and the
included `CHECKSUMS.sha256` afterward.

Run diagnostics and create an index with absolute paths:

```sh
/absolute/path/codeatlas/bin/codeatlas doctor --root /absolute/path/project --json
/absolute/path/codeatlas/bin/codeatlas index --root /absolute/path/project --json
```

Preview a surgical Codex configuration change first:

```sh
/absolute/path/codeatlas/bin/codeatlas integrate codex \
  --root /absolute/path/project \
  --config /absolute/path/to/codex/config.toml \
  --dry-run
```

Repeat with `--apply` only after reviewing the diff. The command writes an absolute
binary path, sets `required = false`, backs up an existing config, and atomically
replaces it. It never changes `PATH`, starts a daemon, downloads a model, or runs a
language SDK.

To remove only the entry created by CodeAtlas:

```sh
/absolute/path/codeatlas/bin/codeatlas integrate codex \
  --config /absolute/path/to/codex/config.toml \
  --remove --dry-run
# Review, then repeat with --remove --apply.
```

Removal does not delete indexes, project memories, source files, or backups. See
`SECURITY_PRIVACY.md` before connecting a remote coding agent: retrieved source can
leave the device through that client even though CodeAtlas itself is local-first.

System dependencies on the qualified build are Apple CoreFoundation/CoreServices,
`libiconv`, and `libSystem`; SQLite, FTS5 and Tree-sitter grammars are compiled into
the executable. Linux and Windows packages are not included or supported yet.
