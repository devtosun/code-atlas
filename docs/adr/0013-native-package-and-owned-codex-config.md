# ADR-0013: deterministic macOS package and owned Codex configuration
Status: accepted
Date: 2026-09-20
Phase: 15

## Context and evidence

The supported release target is macOS ARM64. Codex configuration can contain user
comments, private settings and unrelated MCP definitions, so whole-file generation
or name-based deletion would be unsafe. Startup must remain healthy when CodeAtlas
storage is busy or corrupt.

## Decision

Build the native executable from the committed lockfile and package it in a
deterministic tar/gzip stream with an embedded quick-start, security guide, dependency
notices, build metadata and SHA-256 checksums. Tree-sitter queries remain compile-time
`include_str!` assets and grammars remain statically linked.

Use `toml_edit` for an explicit dry-run/apply/remove CLI. A comment decoration on the
`mcp_servers.codeatlas` table is the ownership marker; no unknown Codex configuration
key is introduced. Reject an unmarked name collision. Apply backs up the original and
atomically replaces it in the same directory. The entry uses absolute canonical paths
and `required = false`. Removal deletes only the marked table and never purges data.

## Alternatives considered

`codex mcp add/remove` does not provide the required comment-preserving ownership and
backup contract. Rewriting the full TOML risks unrelated formatting or values. Adding
a custom ownership field could be rejected by strict Codex config validation. Runtime
`cargo run`, `npx`, download-on-start and a mandatory daemon violate the release model.

## Trade-offs and failure modes

The ownership comment is deliberately visible and can be removed by a user; after
that, update/removal fails safely as a collision. Same-directory atomic replacement is
qualified on macOS ARM64; Windows replacement semantics remain deferred. The package
is unsigned and unnotarized, and deterministic bytes do not establish provenance by
themselves.

## Security and compatibility impact

Dry-run exposes only the owned entry diff. Config symlinks, malformed TOML, relative
paths and non-executable binaries are rejected. Backups are created with restrictive
initial permissions and adopt the original file mode. Optional startup prevents a
busy/corrupt index from making the full Codex session required to fail.

## Tests and rollback strategy

Unit tests cover preserved comments/servers, malformed input, collisions, backups and
owned removal with source/notes sentinels. Extracted-package smoke covers Unicode and
space paths, no language SDKs, both MCP protocol eras, owner/follower, corrupt storage,
EOF and a disposable installed-Codex config parse. Roll back with owned `--remove
--apply` or restore the sibling backup.

## References

- `prompts/15-release-and-codex.md`
- `docs/reports/15-release-and-codex.md`
- `docs/SECURITY_PRIVACY.md`
