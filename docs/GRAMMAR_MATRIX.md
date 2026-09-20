# Grammar compatibility matrix — Phase 00 compatibility through Phase 13 protocol evidence

Tested 2026-09-19 on macOS 27.0 arm64 with Rust 1.98.1, Tree-sitter
0.27.0 and Apple Clang 17.0.0. The runtime accepts grammar ABI 13 through 15.
Every listed package is pinned exactly in `spikes/compatibility/Cargo.toml` and
`Cargo.lock`; crate checksums and source commits are in
`config/dependency-lock.json`.

Phase 03 promoted the same exact versions into the production workspace lock and
loaded every provider through `ca_languages::LanguageRegistry`. `cargo run -p xtask
--locked -- grammar-check` compiles every embedded query, checks the capture contract,
prints package/license/ABI plus BLAKE3 grammar/query fingerprints, and exits nonzero
with language/query context on failure. The grammar fingerprint covers package name,
version, ABI and the package's embedded `node-types.json`; it is a cache/version key,
not a cryptographic attestation of the native object file.

| Target | Published package | Version | Upstream source commit | License | ABI | Basic parse | Query | Native evidence |
|---|---|---:|---|---|---:|---|---|---|
| Dart | `tree-sitter-dart` | 0.2.0 | `nielsenko/tree-sitter-dart@b57d734c84f510bbd524097902cab671e4dbfca9` | MIT | 15 | pass | pass | macOS arm64 |
| C# | `tree-sitter-c-sharp` | 0.23.5 | `tree-sitter/tree-sitter-c-sharp@cac6d5fb595f5811a076336682d5d595ac1c9e85` | MIT | 15 | pass | pass | macOS arm64 |
| Rust | `tree-sitter-rust` | 0.24.2 | `tree-sitter/tree-sitter-rust@e2bee853694a1d3e0f6ef308fe3674542fec95d7` | MIT | 15 | pass | pass | macOS arm64 |
| Go | `tree-sitter-go` | 0.25.0 | `tree-sitter/tree-sitter-go@1547678a9da59885853f5f5cc8a99cc203fa2e2c` | MIT | 15 | pass | pass | macOS arm64 |
| Java | `tree-sitter-java` | 0.23.5 | `tree-sitter/tree-sitter-java@94703d5a6bed02b98e438d7cad1136c01a60ba2c` | MIT | 14 | pass | pass | macOS arm64 |
| JavaScript | `tree-sitter-javascript` | 0.25.0 | `tree-sitter/tree-sitter-javascript@44c892e0be055ac465d5eeddae6d3e194424e7de` | MIT | 15 | pass | pass | macOS arm64 |
| JSX | JavaScript grammar | 0.25.0 | same as JavaScript | MIT | 15 | pass | pass | macOS arm64 |
| TypeScript | `tree-sitter-typescript` | 0.23.2 | `tree-sitter/tree-sitter-typescript@f975a621f4e7f532fe322e13c4f79495e0a7b2e7` | MIT | 14 | pass | pass | macOS arm64 |
| TSX | TypeScript TSX grammar | 0.23.2 | same as TypeScript | MIT | 14 | pass | pass | macOS arm64 |

Current production status for all nine rows: parser and extractor ready, every
provider compiles separate symbols/imports/references/calls assets, and every focused
language suite passes. The provider capability remains syntax-specific; Phase 09's
separate rule-bounded resolver must not be confused with compiler semantics.
TypeScript and TSX use and test distinct grammar values/fingerprints. Reviewed snapshots live in
`fixtures/parser-kernel.golden`, `fixtures/rust-go.golden`, `fixtures/js-ts.golden`,
`fixtures/csharp-java.golden`, and `fixtures/dart.golden`.

Phases 08–10 change no grammar package, ABI, query asset or extractor version. They add
restart-persistent indexing evidence for every row and compare grammar, query and
extractor fingerprints separately before incremental reuse, followed by a separate
generation-scoped resolver pass and syntax-honest retrieval layer.

## Dart provenance and modern syntax

The published `tree-sitter-dart` 0.2.0 archive identifies
`https://github.com/nielsenko/tree-sitter-dart`, author Kasper Overgård Nielsen,
commit `b57d734c84f510bbd524097902cab671e4dbfca9`, and MIT licensing. It is not the
`UserNobody14/tree-sitter-dart` candidate. The separately executed modern fixture
parses extension types, records, relational patterns, and switch expressions with
no ERROR or MISSING nodes. This small fixture is compatibility evidence, not a
claim of complete Dart language coverage. Phase 07 rechecked the packaged
`node-types.json` and representative S-expressions before compiling four production
query assets. The required records, record patterns, sealed classes, extension types
and switch expressions parse without ERROR/MISSING nodes in the reviewed corpus.
Non-ASCII identifiers produce ERROR nodes in this pinned grammar and are explicitly
unsupported; UTF-8 in comments/strings and CRLF byte positions remain tested.

## Protocol, database and platform evidence

- Official `rmcp` 3.4.0, features `server` and `transport-io`, Apache-2.0.
  Subprocess stdio tests pass modern 2026-07-28 `server/discover` plus per-request
  metadata and legacy 2025-11-25 `initialize` / `notifications/initialized`.
  Raw client and server frames are under `spikes/compatibility/artifacts/`.
- Bundled `rusqlite` 0.40.2 with `bundled-full` opened SQLite 3.53.2, above the
  required 3.51.3 floor. `ENABLE_FTS5` is present and an insert/search/delete
  transaction passed. Full compile options are in `config/dependency-lock.json`.
- Installed client recorded: `codex-cli 0.154.0`. The installed client version was
  recorded; the spike does not introspect or claim which lifecycle that external
  client negotiates.
- Linux x64 and Windows x64 MSVC native execution: not run. Cross-compilation is
  not substituted for native evidence.
