# CodeAtlas

Local-first code intelligence for Codex and other MCP clients.

[Türkçe](README.tr.md) · [Command reference](docs/COMMAND_REFERENCE.md) ·
[Security & privacy](docs/SECURITY_PRIVACY.md)

CodeAtlas indexes a source repository and gives your coding agent tools to find
symbols, inspect references and call sites, explore relationships, and retrieve
bounded source context. It is a Rust application using Tree-sitter and a persistent
SQLite/FTS5 index. It does not run an AI model itself.

Currently qualified platform: **macOS ARM64 (Apple Silicon)**. Linux, Windows and
Intel Macs are not qualified. There is no public binary release yet; local packages
are unsigned and not notarized. Native validation was performed on macOS 27.0.

## What is it used for?

Use CodeAtlas to help a coding agent understand a project before explaining code,
investigating a bug or planning a change. Instead of supplying the whole repository
on every request, retrieve relevant evidence with file locations, source hashes and
explicit uncertainty.

- Find functions, classes, methods, fields and other declarations by name.
- Inspect file outlines, references, observed calls and bounded dependency graphs.
- Explore possible change impact and assemble size-limited code context.
- Reuse a persistent index, update it incrementally or opt into file watching.
- Read explicit project notes; enable note writes only with a trusted startup flag.

CodeAtlas runs as one client-launched **stdio MCP process**. It needs no mandatory
daemon, network port, Docker, Redis, Neo4j, embedding service, API key or model
download. The packaged executable needs no Dart, .NET, Go, Java or Node SDK to
analyze source. Your MCP client has its own account/model requirements.

It is not a compiler or language server. Tree-sitter provides syntax evidence;
`syntax_observation`, `lexically_resolved`, `candidate` and `unresolved` distinguish
certainty. Dynamic dispatch, macro expansion, overload selection and unsupported
module mappings are not guessed into confirmed calls. An absent graph edge does not
prove that no dependency exists.

## Supported programming languages

| Language | Indexed extensions | Coverage notes |
|---|---|---|
| Dart | `.dart` | Tested modern Dart and Flutter-style syntax; no analyzer/Flutter SDK integration |
| C# | `.cs` | Namespaces, types, methods and partial-declaration observations; no .NET compiler resolution |
| Rust | `.rs` | Modules, traits, impls and `use` aliases; no macro expansion or active `cfg` selection |
| Go | `.go` | Packages, imports and receiver methods; package binding respects directory boundaries |
| Java | `.java` | Packages, types, methods and imports; no classpath or runtime dispatch resolution |
| JavaScript / JSX | `.js`, `.mjs`, `.cjs`, `.jsx` | ESM and recognized CommonJS; JSX tags are references, not automatic call edges |
| TypeScript / TSX | `.ts`, `.mts`, `.cts`, `.tsx` | Declaration files, type/value roles and a dedicated TSX grammar; no TypeScript type checker |

All seven languages, plus explicit JSX/TSX providers, have parsing, extraction and
negative fixtures. This is not complete ecosystem coverage. The pinned Dart grammar
does not support non-ASCII identifiers. Vue/Svelte, Python, PHP, C/C++, Kotlin and
notebooks are outside this release. See the [language contract](docs/LANGUAGE_SUPPORT.md)
and [grammar matrix](docs/GRAMMAR_MATRIX.md) for precise limits.

## Installation on macOS Apple Silicon

### Build from source

Prerequisites: Git, Rust installed through `rustup`, and Xcode Command Line Tools
or an appropriate Xcode C/C++ toolchain. The repository pins Rust **1.98.1** in
`rust-toolchain.toml`. Building may download dependencies; the default runtime does
not use the network.

```sh
git clone https://github.com/devtosun/code-atlas.git
cd code-atlas
cargo build --release --locked -p ca-cli
./target/release/codeatlas --version
```

With the default Cargo target directory, the executable is `target/release/codeatlas`.
If you set `CARGO_TARGET_DIR`, use its `release/codeatlas` instead. Keep the executable
at a stable absolute path. No global PATH change or administrator installation is
necessary; Python is not a runtime dependency.

### Local package alternative

On a native macOS ARM64 build machine, Python 3.11+ is needed for packaging only:

```sh
cargo fetch --locked
python3 scripts/package_macos.py
python3 scripts/phase15_package_smoke.py \
  dist/codeatlas-0.1.0-aarch64-apple-darwin.tar.gz
```

The package includes the executable, quick start, privacy guide, dependency notices
and checksums. Obtain the archive and its `.sha256` file from a trusted producer;
there is no published download URL to assume. In the directory containing them:

```sh
shasum -a 256 -c codeatlas-0.1.0-aarch64-apple-darwin.tar.gz.sha256
tar -xzf codeatlas-0.1.0-aarch64-apple-darwin.tar.gz
cd codeatlas-0.1.0-aarch64-apple-darwin
shasum -a 256 -c CHECKSUMS.sha256
./bin/codeatlas --version
```

Extract into a new directory, not over an existing installation. Do not bypass
macOS security controls blindly for an unsigned archive.

## First index

Replace both paths below with the executable and the **project you want to analyze**,
not necessarily the CodeAtlas source repository. Both paths must be absolute.

```sh
CODEATLAS_BIN="/absolute/path/to/code-atlas/target/release/codeatlas"
CODEATLAS_PROJECT="/absolute/path/to/your/project"

"$CODEATLAS_BIN" doctor --root "$CODEATLAS_PROJECT" --json
"$CODEATLAS_BIN" index --root "$CODEATLAS_PROJECT" --json
"$CODEATLAS_BIN" status --root "$CODEATLAS_PROJECT" --json
```

`index` reuses unchanged file versions by default. Run it after edits; use
`index --full` to parse all supported source files. Failed/cancelled generations do
not replace a healthy active index. After upgrading to schema v7, explicitly index
to obtain repaired analysis; take a consistent backup before downgrading.

On macOS, each worktree's database lives outside the source tree under
`~/Library/Application Support/CodeAtlas/worktrees/<repository-id>/`. `doctor`
reports the actual path and writer/follower role. Notes survive reindexing.

## Connect to Codex

Codex configures stdio servers with a `[mcp_servers.<name>]` TOML table. See the
[official MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).
CodeAtlas's helper preserves unrelated settings and MCP entries.

### Recommended: preview, then apply

Using the absolute paths from the previous section, preview the owned entry diff:

```sh
"$CODEATLAS_BIN" integrate codex --root "$CODEATLAS_PROJECT" --dry-run
```

Review it, then explicitly apply:

```sh
"$CODEATLAS_BIN" integrate codex --root "$CODEATLAS_PROJECT" --apply
```

The helper targets `$CODEX_HOME/config.toml` when `CODEX_HOME` is set, otherwise
`~/.codex/config.toml`. Add `--config /absolute/path/to/config.toml` to choose another
file. It backs up existing config, writes an absolute binary path, sets
`required = false`, and rejects malformed TOML or unowned name collisions.
It does not change PATH, other servers or global execution settings.

### Manual alternative

Back up your config and merge **only this table** with both paths replaced. Do not
overwrite the whole file or add a duplicate `codeatlas` table.

```toml
[mcp_servers.codeatlas]
command = "/absolute/path/to/code-atlas/target/release/codeatlas"
args = ["serve", "--root", "/absolute/path/to/your/project"]
required = false
startup_timeout_sec = 10
tool_timeout_sec = 60
```

Restart your client/session to load the configuration. With Codex CLI, inspect
configured servers using `codex mcp list`. The client launches `serve`; it is not an
interactive search shell and stdout is reserved for MCP frames.

Example requests:

- “Use CodeAtlas to find this function and inspect its references.”
- “Map this module's observed dependencies and label uncertain relationships.”
- “Build bounded code context for this change before proposing an edit.”

There are **14 default tools**, including `index_repository`, `search_symbols`,
`get_symbol`, `find_references`, `trace_calls`, `read_code`, `analyze_impact`,
`build_context` and `search_memories`. Indexing is explicit, not part of startup.
MCP indexing returns a job ID; use `job_status` and `cancel_job` to track/control it.
See [all tool contracts](docs/MCP_CONTRACT.md).

### Optional watching and note writes

Add `--watch` to the `serve` arguments to keep an existing index fresh while the
writer-owner runs. `--watch-poll` selects polling when watching is enabled. Events
are hints; periodic reconciliation also checks content hashes.

Add `--memory-write` only if you trust the client to author project notes. This
enables `upsert_memory` and `forget_memory` (16 tools total), not permission to edit
source. Both options are off by default.

Only one process can own writes to a worktree database. Followers can read the active
index; writes receive retryable `WRITER_BUSY`. If Codex already owns the writer,
request indexing through that client instead of starting another CLI writer.

## Safety and removal

- Only authorized roots are read. `.gitignore`, `.ignore` and `.codeatlasignore`
  apply; policy excludes symlink escapes, credentials and generated/build output.
- Indexed code, build scripts, hooks, package managers and repository instructions
  are not executed. Source and notes remain untrusted data.
- SQLite storage is plaintext. **Local indexing is not end-to-end private
  inference:** a remote coding agent can receive retrieved source and notes.
- Emitted MCP frames are capped at 65,536 serialized bytes; IDs and inputs also
  have limits. This is not a guarantee of semantic correctness or injection immunity.

Remove an entry created by the integration helper with a preview first:

```sh
"$CODEATLAS_BIN" integrate codex --remove --dry-run
"$CODEATLAS_BIN" integrate codex --remove --apply
```

Include the same `--config` path if you used one. Only the helper's owned entry is
removed, not source, indexes, notes or backups. A manually added unowned table must
be reviewed and removed manually; the helper refuses to claim it.

## Development and evidence

This project began as a phased implementation kit and now includes a working server.
The latest repair run passed 130 workspace tests, formatting, Clippy, all existing
language/corpus gates and native extracted-package tests. Read the exact evidence
and unexecuted checks:

- [Project state](docs/PROJECT_STATE.md) and [test matrix](docs/TEST_MATRIX.md)
- [Review repairs and upgrade notes](docs/reports/review-fixes.md)
- [macOS package report](docs/reports/15-release-and-codex.md)
- [Architecture](docs/ARCHITECTURE.md) and [dependency policy](docs/DEPENDENCY_POLICY.md)
- [Development bootstrap](BOOTSTRAP_PROMPT.tr.md) and [phase manifest](config/phase-manifest.json)
- [Deferred Linux/Windows validation](prompts/14-linux-windows-native-validation.md)
- [License notice](LICENSE-NOTICE.md)

Contributor checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/validate_kit.py
```

Current repairs have not had fresh fuzz/advisory scans or a model-backed Codex
session; previous results remain historical. Signing, notarization, public binary
releases and Linux/Windows qualification are separate work.
