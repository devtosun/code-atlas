#!/usr/bin/env python3
"""Exercise an extracted macOS package in a clean Unicode path and minimal PATH."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

from phase14_measure import McpClient, PROTOCOL_LEGACY, PROTOCOL_MODERN


def run_json(binary: Path, environment: dict[str, str], *arguments: str) -> dict:
    result = subprocess.run(
        [str(binary), *arguments],
        env=environment,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return json.loads(result.stdout)


def run_text(binary: Path, environment: dict[str, str], *arguments: str) -> str:
    return subprocess.run(
        [str(binary), *arguments],
        env=environment,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def safe_extract(archive: Path, destination: Path) -> None:
    with tarfile.open(archive, "r:gz") as bundle:
        root = destination.resolve()
        for member in bundle.getmembers():
            target = (destination / member.name).resolve()
            if root not in target.parents and target != root:
                raise RuntimeError(f"archive member escapes destination: {member.name}")
            if member.issym() or member.islnk():
                raise RuntimeError(f"archive contains a link: {member.name}")
        bundle.extractall(destination, filter="data")


def tool_ok(response: dict, name: str) -> None:
    if "error" in response:
        raise RuntimeError(f"{name} protocol error: {response['error']}")
    result = response.get("result", {})
    if result.get("isError") is True:
        raise RuntimeError(f"{name} application error: {result}")


def protocol_smoke(binary: Path, root: Path, home: Path, modern: bool) -> None:
    client = McpClient(binary, root, home, modern)
    try:
        client.initialize()
        listed = client.request("tools/list", {})
        if len(listed.get("result", {}).get("tools", [])) != 14:
            raise RuntimeError("unexpected default MCP tool count")
        tool_ok(client.tool("repository_status", {}), "repository_status")
        tool_ok(client.tool("search_symbols", {"query": "alpha", "limit": 10}), "search_symbols")
    finally:
        client.stop()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--codex", type=Path)
    arguments = parser.parse_args()
    archive = arguments.archive.resolve()
    expected = archive.with_suffix(archive.suffix + ".sha256").read_text().split()[0]
    actual = hashlib.sha256(archive.read_bytes()).hexdigest()
    if actual != expected:
        raise SystemExit("outer archive checksum mismatch")

    with tempfile.TemporaryDirectory(prefix="CodeAtlas package ğ ") as temporary:
        temporary_path = Path(temporary)
        extraction = temporary_path / "extracted release ü"
        extraction.mkdir()
        safe_extract(archive, extraction)
        binary = next(extraction.glob("codeatlas-*/bin/codeatlas"))
        package_root = binary.parent.parent
        for line in (package_root / "CHECKSUMS.sha256").read_text().splitlines():
            expected_inner, name = line.split("  ", 1)
            candidate = package_root / name
            if (
                not candidate.is_file()
                or hashlib.sha256(candidate.read_bytes()).hexdigest() != expected_inner
            ):
                raise RuntimeError(f"inner package checksum mismatch: {name}")
        empty_path = temporary_path / "empty PATH"
        empty_path.mkdir()
        home = temporary_path / "clean home ş"
        home.mkdir()
        root = temporary_path / "fixture root ı with spaces"
        root.mkdir()
        fixtures = {
            "alpha.rs": "pub fn alpha() -> usize { 1 }\n",
            "beta.go": "package sample\nfunc Beta() int { return 2 }\n",
            "Gamma.java": "class Gamma { int value() { return 3; } }\n",
            "Delta.cs": "class Delta { int Value() { return 4; } }\n",
            "echo.dart": "int echo() => 5;\n",
            "foxtrot.js": "export function foxtrot() { return 6; }\n",
            "golf.ts": "export function golf(): number { return 7; }\n",
        }
        for name, source in fixtures.items():
            (root / name).write_text(source, encoding="utf-8")
        environment = {
            "HOME": str(home),
            "PATH": str(empty_path),
            "TMPDIR": str(temporary_path),
            "LANG": "en_US.UTF-8",
        }
        absent_sdks = ["cargo", "rustc", "node", "go", "java", "dotnet", "dart"]
        if any(shutil.which(name, path=environment["PATH"]) for name in absent_sdks):
            raise RuntimeError("language SDK unexpectedly visible in minimal PATH")

        doctor = run_json(binary, environment, "doctor", "--root", str(root), "--json")
        indexed = run_json(binary, environment, "index", "--root", str(root), "--full", "--json")
        if indexed["files_failed"] != 0 or indexed["files_parsed"] != len(fixtures):
            raise RuntimeError(f"unexpected package index result: {indexed}")

        saved_environment = dict(os.environ)
        os.environ.clear()
        os.environ.update(environment)
        try:
            protocol_smoke(binary, root, home, modern=True)
            protocol_smoke(binary, root, home, modern=False)

            owner = McpClient(binary, root, home, modern=True)
            follower = McpClient(binary, root, home, modern=True)
            try:
                owner.initialize()
                tool_ok(owner.tool("repository_status", {}), "owner status")
                follower.initialize()
                if len(follower.request("tools/list", {}).get("result", {}).get("tools", [])) != 14:
                    raise RuntimeError("busy follower could not list tools")
                tool_ok(follower.tool("repository_status", {}), "follower status")
            finally:
                follower.stop()
                owner.stop()

            corrupt_root = temporary_path / "corrupt index root"
            corrupt_root.mkdir()
            corrupt_doctor = run_json(binary, environment, "doctor", "--root", str(corrupt_root), "--json")
            database = Path(corrupt_doctor["configuration"]["database_path"])
            database.write_bytes(b"not a sqlite database")
            corrupt = McpClient(binary, corrupt_root, home, modern=True)
            try:
                corrupt.initialize()
                if len(corrupt.request("tools/list", {}).get("result", {}).get("tools", [])) != 14:
                    raise RuntimeError("corrupt index blocked tools/list")
            finally:
                corrupt.stop()
        finally:
            os.environ.clear()
            os.environ.update(saved_environment)

        config = temporary_path / "Codex config ç/config.toml"
        config.parent.mkdir()
        original = '# keep\nmodel = "gpt-test"\n\n[mcp_servers.other]\ncommand = "other"\n'
        config.write_text(original, encoding="utf-8")
        dry_run = run_text(
            binary,
            environment,
            "integrate",
            "codex",
            "--root",
            str(root),
            "--config",
            str(config),
            "--dry-run",
        )
        if "required = false" not in dry_run or config.read_text() != original:
            raise RuntimeError("Codex integration dry-run wrote or omitted required=false")
        run_text(
            binary,
            environment,
            "integrate",
            "codex",
            "--root",
            str(root),
            "--config",
            str(config),
            "--apply",
        )
        installed = config.read_text()
        if not installed.startswith(original) or "required = false" not in installed:
            raise RuntimeError("Codex integration did not preserve unrelated config")
        codex_config_parse = "not run"
        codex_version = None
        if arguments.codex:
            codex_environment = {**environment, "CODEX_HOME": str(config.parent)}
            codex_version = subprocess.run(
                [str(arguments.codex), "--version"],
                env=codex_environment,
                check=True,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            ).stdout.strip()
            listed = subprocess.run(
                [str(arguments.codex), "mcp", "list"],
                env=codex_environment,
                check=True,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            ).stdout
            if "codeatlas" not in listed:
                raise RuntimeError("installed Codex did not parse the disposable CodeAtlas entry")
            codex_config_parse = "pass"
        run_text(binary, environment, "integrate", "codex", "--config", str(config), "--remove", "--apply")
        removed = config.read_text()
        if "codeatlas-owned" in removed or "[mcp_servers.other]" not in removed:
            raise RuntimeError("Codex integration removal changed unrelated config")

        report = {
            "schema_version": 1,
            "archive_sha256": actual,
            "inner_checksums": "pass",
            "target": doctor["binary"]["target"],
            "architecture": doctor["runtime"]["architecture"],
            "fixture_files": len(fixtures),
            "files_failed": indexed["files_failed"],
            "protocols": [PROTOCOL_MODERN, PROTOCOL_LEGACY],
            "default_tools": 14,
            "minimal_path_no_language_sdks": True,
            "unicode_space_extraction": "pass",
            "writer_follower": "pass",
            "corrupt_database_did_not_block_tools_list": "pass",
            "eof_shutdown": "pass",
            "codex_config_round_trip": "pass",
            "installed_codex_config_parse": codex_config_parse,
            "installed_codex_version": codex_version,
            "model_backed_codex_session": "not run; no explicit authorization to transmit fixture source",
        }
        encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
        if arguments.output:
            arguments.output.parent.mkdir(parents=True, exist_ok=True)
            arguments.output.write_text(encoded, encoding="utf-8")
        print(encoded, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
