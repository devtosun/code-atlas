# Manual installed-Codex smoke for Phase 15

This optional test sends the selected fixture root and retrieved excerpts to the
Codex service under the signed-in account's data policy. Run it only after explicitly
authorizing that transmission. The automated package smoke does not start a model
session and does not touch the real user config.

1. Verify the package checksum, extract it, and set absolute shell variables for the
   extracted `bin/codeatlas`, a non-private fixture root and the real Codex config.
2. Inspect the installed client without changing configuration:

   ```sh
   codex --version
   codex mcp --help
   "$CODEATLAS_BIN" integrate codex --root "$FIXTURE_ROOT" \
     --config "$CODEX_CONFIG" --dry-run
   ```

3. Review the surgical diff. If and only if the real config change is authorized,
   apply it and confirm Codex parses the entry:

   ```sh
   "$CODEATLAS_BIN" integrate codex --root "$FIXTURE_ROOT" \
     --config "$CODEX_CONFIG" --apply
   codex mcp get codeatlas
   codex mcp list
   ```

4. Start a fresh, ephemeral, read-only session. Ask it to list CodeAtlas tools, call
   `repository_status`, start `index_repository`, poll `job_status`, then exercise
   `search_symbols`, `get_symbol`, `find_references` and `build_context` on the fixture.
   Use `/mcp` in the interactive client if tool discovery needs inspection. Do not
   use a private production root for this test.

   ```sh
   codex exec --ephemeral --sandbox read-only -C "$FIXTURE_ROOT" \
     'Use only the CodeAtlas MCP server. List its tools; index this authorized fixture; poll the job to completion; search for a fixture symbol; fetch it; find its references; build a bounded context bundle; report tool names, job id, generation id, and any errors.'
   ```

5. Run the same command again to prove restart persistence. While one session is
   open, start a second read-only session and call `repository_status`; it must remain
   a healthy query follower rather than blocking Codex startup.
6. Roll back after the test. Review removal before applying it:

   ```sh
   "$CODEATLAS_BIN" integrate codex --config "$CODEX_CONFIG" --remove --dry-run
   "$CODEATLAS_BIN" integrate codex --config "$CODEX_CONFIG" --remove --apply
   codex mcp list
   ```

The removal must leave source, indexes, project memories and other MCP definitions
unchanged. Retain the timestamped backup until the config is verified. Restore that
backup manually if a later client-specific issue appears.
