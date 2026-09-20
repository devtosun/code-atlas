#!/usr/bin/env python3
"""Record a reproducible, offline dependency and license inventory for Phase 14."""

from __future__ import annotations

import argparse
import json
import subprocess
from collections import defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


def command(arguments: list[str]) -> dict[str, Any]:
    process = subprocess.run(arguments, capture_output=True, text=True, timeout=120)
    return {
        "command": " ".join(arguments),
        "exit_code": process.returncode,
        "stdout": process.stdout.strip().replace(str(ROOT), "<workspace>"),
        "stderr": process.stderr.strip().replace(str(ROOT), "<workspace>"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()

    rustc = command(["rustc", "-vV"])
    host = next(
        (line.removeprefix("host: ") for line in rustc["stdout"].splitlines() if line.startswith("host: ")),
        None,
    )
    if rustc["exit_code"] != 0 or not host:
        raise SystemExit(rustc["stderr"] or "rustc did not report a host target")
    metadata_arguments = [
        "cargo",
        "metadata",
        "--locked",
        "--offline",
        "--filter-platform",
        host,
        "--format-version",
        "1",
    ]
    metadata_run = command(metadata_arguments)
    if metadata_run["exit_code"] != 0:
        raise SystemExit(metadata_run["stderr"])
    metadata = json.loads(metadata_run["stdout"])
    workspace = set(metadata["workspace_members"])
    third_party = [package for package in metadata["packages"] if package["id"] not in workspace]
    packages = [
        {
            "name": package["name"],
            "version": package["version"],
            "source": package["source"],
            "license": package["license"],
            "license_file": package["license_file"],
        }
        for package in sorted(third_party, key=lambda item: (item["name"], item["version"]))
    ]
    by_name: dict[str, list[str]] = defaultdict(list)
    for package in packages:
        by_name[package["name"]].append(package["version"])
    duplicates = {
        name: versions for name, versions in sorted(by_name.items()) if len(set(versions)) > 1
    }
    missing_license = [
        f"{package['name']} {package['version']}"
        for package in packages
        if not package["license"] and not package["license_file"]
    ]
    non_registry_sources = [
        package
        for package in packages
        if not str(package["source"]).startswith("registry+")
    ]
    tool_checks = {
        "cargo_audit": command(["cargo", "audit", "--version"]),
        "cargo_deny": command(["cargo", "deny", "--version"]),
        "cargo_fuzz": command(["cargo", "fuzz", "--version"]),
        "nightly": command(["cargo", "+nightly", "--version"]),
        "fuzz_manifest_offline": command(
            ["cargo", "check", "--manifest-path", "fuzz/Cargo.toml", "--offline"]
        ),
    }
    advisory_scan = command(
        ["cargo", "audit", "--no-fetch", "--deny", "warnings", "--json"]
    )
    policy_scan = command(
        [
            "cargo",
            "deny",
            "--locked",
            "--offline",
            "--format",
            "json",
            "check",
        ]
    )
    evidence = {
        "schema_version": 1,
        "metadata_command": " ".join(metadata_arguments),
        "target_scope": host,
        "workspace_packages": len(workspace),
        "third_party_packages": len(packages),
        "packages": packages,
        "duplicate_versions": duplicates,
        "missing_third_party_license_metadata": missing_license,
        "non_registry_third_party_sources": non_registry_sources,
        "tool_checks": tool_checks,
        "advisory_scan": advisory_scan,
        "dependency_policy_scan": policy_scan,
        "conclusion": {
            "locked_offline_metadata": "pass",
            "license_metadata_complete": "pass" if not missing_license else "blocked",
            "unexpected_git_or_path_dependency": "pass" if not non_registry_sources else "blocked",
            "advisory_scan": "pass" if advisory_scan["exit_code"] == 0 else "blocked",
            "license_policy_scan": "pass" if policy_scan["exit_code"] == 0 else "blocked",
            "fuzz_execution": "not_run"
            if tool_checks["cargo_fuzz"]["exit_code"] or tool_checks["nightly"]["exit_code"]
            else "available",
        },
    }
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
