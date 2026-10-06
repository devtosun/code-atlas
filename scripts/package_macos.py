#!/usr/bin/env python3
"""Build a deterministic local macOS ARM64 CodeAtlas archive from Cargo.lock."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import subprocess
import tarfile


ROOT = Path(__file__).resolve().parents[1]
TARGET = "aarch64-apple-darwin"
VERSION = "0.1.0"
PACKAGE = f"codeatlas-{VERSION}-{TARGET}"


def command_output(arguments: list[str]) -> str:
    return subprocess.run(
        arguments,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def dependency_notices(cargo: str) -> bytes:
    metadata = json.loads(
        command_output(
            [cargo, "metadata", "--locked", "--offline", "--format-version=1"]
        )
    )
    rows: set[tuple[str, str, str, str]] = set()
    for package in metadata["packages"]:
        if package.get("source") is None:
            continue
        rows.add(
            (
                package["name"],
                package["version"],
                package.get("license") or "license-file; see upstream package",
                package.get("repository") or package.get("homepage") or package["source"],
            )
        )
    lines = [
        "# Third-party notices",
        "",
        "Generated from the locked Cargo metadata used for this package. License",
        "identifiers are upstream declarations; the Phase 14 cargo-deny policy is the",
        "reviewed license gate. Source packages remain available from crates.io.",
        "",
        "| Package | Version | License | Upstream |",
        "|---|---:|---|---|",
    ]
    for name, version, license_name, upstream in sorted(rows):
        lines.append(f"| `{name}` | `{version}` | `{license_name}` | {upstream} |")
    lines.append("")
    return "\n".join(lines).encode()


def deterministic_archive(output: Path, files: dict[str, tuple[bytes, int]]) -> None:
    with output.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as tar:
                directories = {PACKAGE, *(str(Path(PACKAGE, name).parent) for name in files)}
                for directory in sorted(directories):
                    info = tarfile.TarInfo(directory)
                    info.type = tarfile.DIRTYPE
                    info.mode = 0o755
                    info.mtime = 0
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    tar.addfile(info)
                for name in sorted(files):
                    data, mode = files[name]
                    info = tarfile.TarInfo(str(Path(PACKAGE, name)))
                    info.size = len(data)
                    info.mode = mode
                    info.mtime = 0
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    tar.addfile(info, io.BytesIO(data))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--skip-build", action="store_true")
    arguments = parser.parse_args()

    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise SystemExit("packaging requires native Darwin arm64 execution")
    cargo = os.environ.get("CARGO", "cargo")
    if not arguments.skip_build:
        subprocess.run(
            [cargo, "build", "--release", "--locked", "--offline", "-p", "ca-cli"],
            cwd=ROOT,
            check=True,
        )
    target_directory = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
    if not target_directory.is_absolute():
        target_directory = ROOT / target_directory
    binary = target_directory / "release/codeatlas"
    if not binary.is_file():
        raise SystemExit(f"missing release binary: {binary}")
    binary_kind = command_output(["/usr/bin/file", str(binary)])
    if "Mach-O 64-bit executable arm64" not in binary_kind:
        raise SystemExit(f"unexpected binary target: {binary_kind}")
    dynamic = command_output(["/usr/bin/otool", "-L", str(binary)]).splitlines()[1:]
    allowed = ("/System/Library/", "/usr/lib/")
    dependencies = [line.strip().split(" ", 1)[0] for line in dynamic if line.strip()]
    if not dependencies or any(not item.startswith(allowed) for item in dependencies):
        raise SystemExit(f"unexpected dynamic dependency: {dependencies}")

    files: dict[str, tuple[bytes, int]] = {
        "bin/codeatlas": (binary.read_bytes(), 0o755),
        "QUICKSTART.md": ((ROOT / "docs/release/QUICKSTART.md").read_bytes(), 0o644),
        "LICENSE-NOTICE.md": ((ROOT / "LICENSE-NOTICE.md").read_bytes(), 0o644),
        "SECURITY_PRIVACY.md": ((ROOT / "docs/SECURITY_PRIVACY.md").read_bytes(), 0o644),
        "THIRD_PARTY_NOTICES.md": (dependency_notices(cargo), 0o644),
    }
    build_metadata = {
        "schema_version": 1,
        "package": PACKAGE,
        "target": TARGET,
        "rustc": command_output(["rustc", "--version"]),
        "dynamic_dependencies": dependencies,
        "embedded_assets": "Tree-sitter queries are compiled with include_str! and grammars are statically linked",
        "signing": "unsigned",
        "notarization": "not run",
    }
    files["BUILD-METADATA.json"] = (
        (json.dumps(build_metadata, indent=2, sort_keys=True) + "\n").encode(),
        0o644,
    )
    checksum_lines = [
        f"{sha256_bytes(data)}  {name}" for name, (data, _) in sorted(files.items())
    ]
    files["CHECKSUMS.sha256"] = (("\n".join(checksum_lines) + "\n").encode(), 0o644)

    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"{PACKAGE}.tar.gz"
    deterministic_archive(archive, files)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum = archive.with_suffix(archive.suffix + ".sha256")
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(json.dumps({"archive": str(archive), "sha256": digest, "checksum": str(checksum)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
