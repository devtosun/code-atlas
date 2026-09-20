# Research sources and design provenance

Research date: 2026-09-19. Links below are primary documentation or upstream source.
Facts from documentation are not proof that this unimplemented kit has passed them.
Use exact pinned releases during implementation; main/latest URLs can change.

| ID | Source | Relevance |
|---|---|---|
| S01 | https://github.com/DeusData/codebase-memory-mcp | Functional inspiration only; do not copy benchmark claims or code wholesale |
| S02 | https://developers.openai.com/codex/skills/ | .agents/skills, SKILL.md frontmatter, explicit $skill invocation |
| S03 | https://developers.openai.com/codex/guides/agents-md/ | Repository instructions and discovery |
| S04 | https://developers.openai.com/codex/mcp/ | Codex config, stdio, required flag, startup/tool timeouts |
| S05 | https://github.com/modelcontextprotocol/rust-sdk | Official rmcp SDK; current/legacy lifecycle and schemas |
| S06 | https://modelcontextprotocol.io/docs/2026-07-28/learn/versioning | Current protocol and compatibility model |
| S07 | https://docs.rs/tree-sitter/latest/tree_sitter/ | Runtime, ABI interval, queries and incremental parsing |
| S08 | https://docs.rs/tree-sitter-dart/latest/tree_sitter_dart/ | Published Dart binding; verify package provenance separately |
| S09 | https://github.com/UserNobody14/tree-sitter-dart | Alternative Dart upstream candidate; not assumed identical to S08 |
| S10 | https://github.com/tree-sitter/tree-sitter-c-sharp | C# grammar source |
| S11 | https://github.com/tree-sitter/tree-sitter-rust | Rust grammar source |
| S12 | https://github.com/tree-sitter/tree-sitter-go | Go grammar source |
| S13 | https://github.com/tree-sitter/tree-sitter-java | Java grammar source |
| S14 | https://github.com/tree-sitter/tree-sitter-javascript | JavaScript grammar source |
| S15 | https://github.com/tree-sitter/tree-sitter-typescript | Separate TypeScript/TSX grammars |
| S16 | https://sqlite.org/wal.html | WAL behavior, single writer, local storage, WAL-reset fix |
| S17 | https://sqlite.org/fts5.html | FTS5 retrieval and transactional maintenance |
| S18 | https://docs.rs/notify/latest/notify/ | Watcher differences, lost events, polling fallback |
| S19 | https://docs.rs/ignore/latest/ignore/ | Ignore-aware traversal |
| S20 | https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html | Blocking pool limits and non-abortable started work |
| S21 | https://docs.rs/rusqlite/latest/rusqlite/ | SQLite Rust adapter |
| S22 | https://docs.rs/gix/latest/gix/ | Git metadata library |
| S23 | https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio | Current stdio transport |
| S24 | https://modelcontextprotocol.io/specification/2026-07-28/server/tools | Current schemas, structured results and errors |
| S25 | https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning | Modern/legacy interoperability |
| S26 | https://modelcontextprotocol.io/specification/2025-11-25/basic/transports | Legacy transport baseline |
| S27 | https://modelcontextprotocol.io/specification/2025-11-25/server/tools | Legacy tool-result baseline |

## Specific research cautions
The official MCP versioning page identifies 2026-07-28 as current. Its modern
per-request model differs from the older initialize-based model; rmcp's current
README describes compatibility with both. Check the published version, not just main.

The inspected UserNobody14 Dart Cargo manifest and the published Dart crate docs
expose different binding-generation styles. A name match does not establish common
maintainer, version, ABI or syntax support. Phase 00 resolved the published crate to
`nielsenko/tree-sitter-dart`; see the grammar matrix for exact evidence.

SQLite's official WAL documentation identifies a corruption fix beginning at
3.51.3, with named backports. The bundled library version must be verified before
using a multiple-connection WAL design. This is a required verification, not an
assumption that a recent Rust crate necessarily bundles the patched library.

All proposed budgets, architecture, stage order, fixture content and acceptance
criteria in this kit are design choices. No upstream benchmark number is adopted
as a performance promise for CodeAtlas.
