# Manual Codex smoke recipe for Phase 11

This recipe is intentionally manual and does not edit the user's Codex
configuration. Phase 15 owns persistent, backed-up configuration.

## Preconditions

Build the binary and run its wire suite:

    cargo build -p ca-cli --locked
    cargo test -p ca-cli --test lifecycle --locked

Choose an absolute source root that may be exposed to the Codex client/model through
MCP results. The index is local, but returned source snippets can reach a remote
model. Record the client with:

    codex --version

## One-session configuration override

Replace both paths below. The -c values apply only to this invocation and do not
write config.toml.

    codex \
      -C /absolute/path/to/project \
      -c 'mcp_servers.codeatlas.command="/absolute/path/to/codeatlas-mcp-codex-kit/target/debug/codeatlas"' \
      -c 'mcp_servers.codeatlas.args=["serve","--root","/absolute/path/to/project"]' \
      -c 'mcp_servers.codeatlas.required=false' \
      -c 'mcp_servers.codeatlas.startup_timeout_sec=10' \
      -c 'mcp_servers.codeatlas.tool_timeout_sec=60'

In the session, ask Codex to:

1. call repository_status and confirm not_opened;
2. call index_repository in incremental mode and retain its application job_id;
3. poll job_status until a real terminal state is returned;
4. call search_symbols, then get_symbol, get_file_outline and read_code using the
   returned symbol/path/hash evidence;
5. call find_references, trace_calls, get_repo_map, analyze_impact and build_context;
6. confirm candidate edges remain opt-in and that the tools do not claim MCP Tasks.

Close the Codex session and confirm the client-launched server exits. If it closes
while indexing, restart with the same root and check that the durable job is terminal
and no incomplete candidate replaced a prior healthy generation.

## Phase 11 execution record

- Installed client detected: codex-cli 0.154.0.
- codex --help, codex mcp --help and codex mcp add --help ran successfully on
  2026-09-19.
- Actual installed-Codex MCP session: **not run**. No user configuration was changed,
  no model session was started and SDK/subprocess tests are not presented as
  real-client verification.
