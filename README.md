# CodeAtlas MCP — Codex development kit

Start with [Türkçe kullanım rehberi](README.tr.md) and the
[ready-to-paste bootstrap prompt](BOOTSTRAP_PROMPT.tr.md).

This repository began as a specification and phased Codex development kit. Through
Phase 14 it now also contains a working Rust application with sixteen typed MCP
server, safe repository/storage boundary, all required syntax extractors and a
persistent indexing CLI with bounded lexical/import resolution and a generation-
scoped evidence graph. Status, asynchronous index jobs, bounded retrieval, graph
queries, source reads and context assembly are exposed through official rmcp.
An opt-in owner-only watcher uses bounded event hints plus periodic scan/hash
reconciliation. Explicit revisioned project memories, typed resources and static
client prompt templates are implemented. The macOS hardening/evaluation run is
recorded, and the repaired lookup and post-debounce edit performance targets pass.
Phase 14 is complete for the supported macOS ARM64 target after bounded fuzz runs,
supply-chain scans, exhaustive accuracy labels and stripped-release validation.
Linux and Windows remain deferred and unsupported; Phase 15 packaging/integration
has not yet run, so this is not a published-release claim.

Implemented commands are `codeatlas serve --root <absolute-path> [--watch]
[--memory-write]`, `doctor`, `index` and `status`; `serve` advertises fourteen tools
by default and all sixteen under trusted memory-write opt-in. Parsing targets Dart,
C#, Rust, Go, Java, JavaScript and
TypeScript, including explicit JSX and TSX modes. See `docs/PROJECT_STATE.md` and
`docs/reports/14-hardening-and-evaluation.md` for exact evidence and limitations.
Use `prompts/14-linux-windows-native-validation.md` for the deferred native matrix.

Research baseline: 2026-09-19. Phase 00 dependency evidence is pinned in
`config/dependency-lock.json` and `docs/GRAMMAR_MATRIX.md`.
Sources and version-sensitive caveats are in [docs/SOURCES.md](docs/SOURCES.md).
Static package verification: `python3 scripts/validate_kit.py` (Python 3.11+).
Python is only a validator for this text kit, not a planned application dependency.
