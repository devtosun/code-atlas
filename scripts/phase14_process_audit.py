#!/usr/bin/env python3
"""Inspect a live default CodeAtlas stdio server for network sockets."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

from phase14_measure import McpClient, sha256


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    binary = arguments.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit(f"release binary is not executable: {binary}")

    with tempfile.TemporaryDirectory(prefix="codeatlas-phase14-process-") as temporary:
        base = Path(temporary)
        root = base / "root"
        home = base / "home"
        root.mkdir()
        home.mkdir()
        client = McpClient(binary, root, home, modern=True)
        try:
            client.initialize()
            listed = client.request("tools/list", {})
            status = client.tool("repository_status", {})
            process = subprocess.run(
                ["/usr/sbin/lsof", "-nP", "-a", "-p", str(client.process.pid), "-i"],
                capture_output=True,
                text=True,
                timeout=30,
            )
            sockets = [line for line in process.stdout.splitlines() if line.strip()]
            if sockets:
                raise RuntimeError("default server opened a network socket")
            evidence = {
                "schema_version": 1,
                "binary_source": str(arguments.binary),
                "binary_sha256": sha256(binary),
                "protocol": "2026-07-28",
                "advertised_tools": len(listed.get("result", {}).get("tools", [])),
                "runtime_network_field": status.get("result", {})
                .get("structuredContent", {})
                .get("data", {})
                .get("runtime_network"),
                "inspection_command": "lsof -nP -a -p <server-pid> -i",
                "lsof_exit_code": process.returncode,
                "network_socket_rows": sockets,
                "conclusion": "pass_no_network_sockets",
            }
        finally:
            shutdown_ms = client.stop()
        evidence["eof_shutdown_ms"] = shutdown_ms

    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
