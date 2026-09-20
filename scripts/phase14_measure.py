#!/usr/bin/env python3
"""Measure a packaged CodeAtlas release binary on generated, non-private corpora."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import select
import shutil
import statistics
import subprocess
import tempfile
import time
from typing import Any

PROTOCOL_MODERN = "2026-07-28"
PROTOCOL_LEGACY = "2025-11-25"
FRAME_LIMIT = 65_536


def percentile(samples: list[float], quantile: float) -> float:
    ordered = sorted(samples)
    if not ordered:
        raise ValueError("cannot compute a percentile without samples")
    index = max(0, min(len(ordered) - 1, int(len(ordered) * quantile + 0.999999) - 1))
    return ordered[index]


def summary_ms(samples: list[float]) -> dict[str, Any]:
    return {
        "samples": len(samples),
        "minimum_ms": min(samples),
        "p50_ms": percentile(samples, 0.50),
        "p95_ms": percentile(samples, 0.95),
        "maximum_ms": max(samples),
        "mean_ms": statistics.fmean(samples),
        "raw_ms": samples,
    }


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def modern_meta() -> dict[str, Any]:
    return {
        "io.modelcontextprotocol/protocolVersion": PROTOCOL_MODERN,
        "io.modelcontextprotocol/clientInfo": {
            "name": "codeatlas-phase-14-measure",
            "version": "0.1.0",
        },
        "io.modelcontextprotocol/clientCapabilities": {},
    }


class McpClient:
    def __init__(
        self,
        binary: Path,
        root: Path,
        home: Path,
        modern: bool,
        serve_arguments: list[str] | None = None,
    ) -> None:
        self.modern = modern
        self.next_id = 1
        self.process = subprocess.Popen(
            [
                str(binary),
                "serve",
                "--root",
                str(root),
                *(serve_arguments or []),
            ],
            env={**os.environ, "HOME": str(home)},
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )

    def _send(self, frame: dict[str, Any]) -> None:
        if self.process.stdin is None:
            raise RuntimeError("MCP stdin is closed")
        self.process.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.process.stdin.flush()

    def _receive(self, timeout: float = 30.0) -> dict[str, Any]:
        if self.process.stdout is None:
            raise RuntimeError("MCP stdout is closed")
        ready, _, _ = select.select([self.process.stdout], [], [], timeout)
        if not ready:
            raise TimeoutError("MCP response exceeded measurement timeout")
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited before response: {stderr}")
        encoded = line.encode("utf-8")
        if len(encoded) > FRAME_LIMIT:
            raise RuntimeError(f"MCP frame exceeded {FRAME_LIMIT} bytes")
        return json.loads(line)

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        request_id = self.next_id
        self.next_id += 1
        if self.modern:
            params = {**params, "_meta": modern_meta()}
        self._send(
            {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        )
        response = self._receive()
        if response.get("id") != request_id:
            raise RuntimeError("MCP response ID mismatch")
        return response

    def initialize(self) -> None:
        if self.modern:
            response = self.request("server/discover", {})
            versions = response.get("result", {}).get("supportedVersions", [])
            if PROTOCOL_MODERN not in versions:
                raise RuntimeError("modern protocol was not discovered")
        else:
            response = self.request(
                "initialize",
                {
                    "protocolVersion": PROTOCOL_LEGACY,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "codeatlas-phase-14-measure",
                        "version": "0.1.0",
                    },
                },
            )
            if response.get("result", {}).get("protocolVersion") != PROTOCOL_LEGACY:
                raise RuntimeError("legacy protocol negotiation failed")
            self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def stop(self) -> float:
        started = time.perf_counter()
        if self.process.stdin is not None:
            self.process.stdin.close()
        self.process.wait(timeout=30)
        elapsed = (time.perf_counter() - started) * 1000
        if self.process.returncode != 0:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP shutdown failed: {stderr}")
        return elapsed


def startup_samples(
    binary: Path, root: Path, home: Path, modern: bool, count: int
) -> dict[str, Any]:
    samples: list[float] = []
    shutdown: list[float] = []
    for _ in range(count):
        started = time.perf_counter()
        client = McpClient(binary, root, home, modern)
        client.initialize()
        listed = client.request("tools/list", {})
        if len(listed.get("result", {}).get("tools", [])) != 14:
            raise RuntimeError("unexpected default tool count")
        samples.append((time.perf_counter() - started) * 1000)
        shutdown.append(client.stop())
    return {
        "protocol": PROTOCOL_MODERN if modern else PROTOCOL_LEGACY,
        "spawn_through_tools_list": summary_ms(samples),
        "eof_shutdown": summary_ms(shutdown),
    }


def generate_corpus(root: Path, file_count: int) -> int:
    total_bytes = 0
    for index in range(file_count):
        shard = root / f"shard-{index // 1000:03d}"
        shard.mkdir(exist_ok=True)
        source = f"pub fn symbol_{index:06d}() -> usize {{ {index} }}\n"
        encoded = source.encode("utf-8")
        (shard / f"unit-{index:06d}.rs").write_bytes(encoded)
        total_bytes += len(encoded)
    return total_bytes


def timed_json(binary: Path, home: Path, arguments: list[str]) -> dict[str, Any]:
    command = [str(binary), *arguments]
    started = time.perf_counter()
    process = subprocess.Popen(
        command,
        env={**os.environ, "HOME": str(home)},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    peak_rss_bytes: int | None = None
    deadline = time.monotonic() + 900
    while process.poll() is None:
        if time.monotonic() >= deadline:
            process.kill()
            process.wait()
            raise TimeoutError(f"command exceeded 900 seconds: {' '.join(command)}")
        rss = command_output(["/bin/ps", "-o", "rss=", "-p", str(process.pid)])
        if rss:
            try:
                rss_bytes = int(rss) * 1024
                peak_rss_bytes = max(peak_rss_bytes or 0, rss_bytes)
            except ValueError:
                pass
        time.sleep(0.005)
    stdout, stderr = process.communicate()
    elapsed = time.perf_counter() - started
    if process.returncode != 0:
        raise RuntimeError(
            f"command failed ({process.returncode}): {' '.join(command)}\n{stderr}"
        )
    return {
        "seconds": elapsed,
        "peak_rss_bytes": peak_rss_bytes,
        "result": json.loads(stdout),
    }


def measure_corpus(binary: Path, base: Path, file_count: int) -> dict[str, Any]:
    root = base / f"corpus-{file_count}"
    home = base / f"home-{file_count}"
    root.mkdir()
    home.mkdir()
    source_bytes = generate_corpus(root, file_count)
    full = timed_json(binary, home, ["index", "--root", str(root), "--full", "--json"])
    no_op = timed_json(binary, home, ["index", "--root", str(root), "--json"])
    edit_path = root / "shard-000" / "unit-000000.rs"
    edit_path.write_text("pub fn symbol_000000() -> usize { 999_999 }\n", encoding="utf-8")
    single_edit = timed_json(binary, home, ["index", "--root", str(root), "--json"])
    parsed = full["result"].get("files_parsed", 0)
    return {
        "kind": "deterministic_generated_rust_v1",
        "authorization": "generated locally for this evaluation; no private source",
        "files": file_count,
        "source_bytes": source_bytes,
        "expected_declarations": file_count,
        "generation_rule": "one unique public Rust function per file, 1000 files per shard",
        "full_index": {
            **full,
            "files_per_second": parsed / full["seconds"] if full["seconds"] else None,
        },
        "no_op_update": no_op,
        "single_file_edit": single_edit,
    }


def tool_measurements(
    binary: Path, root: Path, home: Path, file_count: int, lookup_samples: int
) -> dict[str, Any]:
    client = McpClient(binary, root, home, modern=True)
    client.initialize()
    lookup_ms: list[float] = []
    query = f"symbol_{file_count - 1:06d}"
    for _ in range(lookup_samples):
        started = time.perf_counter()
        response = client.tool("search_symbols", {"query": query, "limit": 20})
        lookup_ms.append((time.perf_counter() - started) * 1000)
        result = response.get("result", {})
        if result.get("isError") or not result.get("structuredContent", {}).get("data", {}).get(
            "results"
        ):
            raise RuntimeError("warm exact lookup did not return the generated symbol")
    indexed_status_response = client.tool("repository_status", {})
    indexed_status = (
        indexed_status_response.get("result", {}).get("structuredContent", {}).get("data", {})
    )

    index = client.tool(
        "index_repository",
        {"mode": "full", "request_key": "phase-14-active-cancellation"},
    )
    job_id = index["result"]["structuredContent"]["data"]["job_id"]
    deadline = time.monotonic() + 60
    observed_active = False
    while time.monotonic() < deadline:
        status = client.tool("job_status", {"job_id": job_id})
        state = status["result"]["structuredContent"]["data"]["state"]
        if state in {"scanning", "parsing", "resolving", "committing"}:
            observed_active = True
            break
        if state in {"completed", "cancelled", "failed", "interrupted"}:
            break
        time.sleep(0.005)

    status_ms: list[float] = []
    if observed_active:
        for _ in range(20):
            started = time.perf_counter()
            response = client.tool("repository_status", {})
            status_ms.append((time.perf_counter() - started) * 1000)
            if response.get("result", {}).get("isError"):
                raise RuntimeError("status failed while indexing")

    cancel_started = time.perf_counter()
    cancel = client.tool("cancel_job", {"job_id": job_id})
    cancel_ack_ms = (time.perf_counter() - cancel_started) * 1000
    terminal_state = cancel["result"]["structuredContent"]["status"]
    deadline = time.monotonic() + 60
    while terminal_state not in {"cancelled", "completed", "failed", "interrupted"}:
        if time.monotonic() >= deadline:
            raise TimeoutError("cancelled job did not stop within 60 seconds")
        status = client.tool("job_status", {"job_id": job_id})
        terminal_state = status["result"]["structuredContent"]["data"]["state"]
        time.sleep(0.005)
    cancel_terminal_ms = (time.perf_counter() - cancel_started) * 1000
    shutdown_ms = client.stop()
    return {
        "indexed_corpus_status": indexed_status,
        "warm_exact_symbol_lookup": summary_ms(lookup_ms),
        "active_index_observed": observed_active,
        "status_while_indexing": summary_ms(status_ms) if status_ms else None,
        "cancellation": {
            "ack_ms": cancel_ack_ms,
            "terminal_ms": cancel_terminal_ms,
            "terminal_state": terminal_state,
        },
        "shutdown_after_work_ms": shutdown_ms,
    }


def watch_edit_measurements(
    binary: Path,
    root: Path,
    home: Path,
    samples: int,
    debounce_ms: int = 250,
) -> dict[str, Any]:
    client = McpClient(
        binary,
        root,
        home,
        modern=True,
        serve_arguments=[
            "--watch",
            "--watch-debounce-ms",
            str(debounce_ms),
            "--watch-poll",
            "--watch-reconcile-seconds",
            "60",
        ],
    )
    client.initialize()
    opened = client.tool("search_symbols", {"query": "symbol_000000", "limit": 1})
    if opened.get("result", {}).get("isError"):
        raise RuntimeError("watch measurement could not open the indexed repository")
    status = client.tool("repository_status", {})
    data = status["result"]["structuredContent"]["data"]
    generation = status["result"]["structuredContent"].get("generation_id")
    watch = data.get("watch", {})
    start_deadline = time.monotonic() + 5
    while not watch.get("running") and time.monotonic() < start_deadline:
        time.sleep(0.01)
        status = client.tool("repository_status", {})
        data = status["result"]["structuredContent"]["data"]
        generation = status["result"]["structuredContent"].get("generation_id")
        watch = data.get("watch", {})
    if not watch.get("running"):
        raise RuntimeError(f"watcher did not start: {watch}")
    time.sleep(0.5)

    edit_path = root / "shard-000" / "unit-000000.rs"
    event_to_activation_ms: list[float] = []
    after_debounce_ms: list[float] = []
    jobs: list[dict[str, Any]] = []
    edit_base = 1_000_000 + time.time_ns() % 1_000_000_000
    for index in range(samples):
        reconciliation_count = watch.get("reconciliation_count", 0)
        edit_path.write_text(
            f"pub fn symbol_000000() -> usize {{ {edit_base + index} }}\n",
            encoding="utf-8",
        )
        started = time.perf_counter()
        deadline = time.monotonic() + 30
        while True:
            response = client.tool("repository_status", {})
            envelope = response["result"]["structuredContent"]
            current = envelope.get("generation_id")
            current_data = envelope.get("data", {})
            latest_job = current_data.get("latest_job")
            watch = current_data.get("watch", {})
            if (
                current
                and current != generation
                and latest_job
                and watch.get("reconciliation_count", 0) > reconciliation_count
                and watch.get("last_reconciliation_duration_ms") is not None
            ):
                generation = current
                jobs.append(latest_job)
                break
            if time.monotonic() >= deadline:
                raise TimeoutError(
                    "watch edit did not activate within 30 seconds: "
                    f"watch={current_data.get('watch')} latest_job={latest_job}"
                )
            time.sleep(0.005)
        elapsed_ms = (time.perf_counter() - started) * 1000
        event_to_activation_ms.append(elapsed_ms)
        after_debounce_ms.append(float(watch["last_reconciliation_duration_ms"]))
        progress = latest_job.get("progress", {})
        if progress.get("files_parsed") != 1 or progress.get("files_deleted") != 0:
            raise RuntimeError(f"watch edit was not targeted to one file: {latest_job}")

    final_status = client.tool("repository_status", {})
    shutdown_ms = client.stop()
    return {
        "fixture_files": 10_000,
        "samples": samples,
        "debounce_ms": debounce_ms,
        "measurement_boundary": {
            "event_to_activation": "source write through active generation publication",
            "after_debounce": "measured reconciliation callback wall clock after debounce",
        },
        "event_to_activation": summary_ms(event_to_activation_ms),
        "after_debounce": summary_ms(after_debounce_ms),
        "jobs": jobs,
        "final_watch_status": final_status["result"]["structuredContent"]["data"].get(
            "watch"
        ),
        "shutdown_ms": shutdown_ms,
    }


def command_output(arguments: list[str]) -> str | None:
    try:
        result = subprocess.run(arguments, capture_output=True, text=True, timeout=30, check=True)
        return result.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None


def hardware_summary() -> dict[str, str] | None:
    output = command_output(["system_profiler", "SPHardwareDataType"])
    if output is None:
        return None
    allowed = {"Model Name", "Model Identifier", "Chip", "Total Number of Cores", "Memory"}
    summary: dict[str, str] = {}
    for line in output.splitlines():
        key, separator, value = line.strip().partition(":")
        if separator and key in allowed:
            summary[key] = value.strip()
    return summary or None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--startup-samples", type=int, default=30)
    parser.add_argument("--lookup-samples", type=int, default=50)
    parser.add_argument("--watch-edit-samples", type=int, default=10)
    parser.add_argument("--file-counts", type=int, nargs="+", default=[10_000, 100_000])
    arguments = parser.parse_args()
    binary_label = str(arguments.binary)
    binary = arguments.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit(f"release binary is not executable: {binary}")
    if (
        arguments.startup_samples < 1
        or arguments.lookup_samples < 1
        or arguments.watch_edit_samples < 1
    ):
        raise SystemExit("sample counts must be positive")
    if not arguments.file_counts or any(count < 1 or count > 100_000 for count in arguments.file_counts):
        raise SystemExit("file counts must be in 1..=100000")

    with tempfile.TemporaryDirectory(prefix="codeatlas-phase14-") as temporary:
        base = Path(temporary)
        package = base / "package"
        package.mkdir()
        packaged_binary = package / "codeatlas"
        shutil.copy2(binary, packaged_binary)
        packaged_binary.chmod(0o755)
        empty_root = base / "empty-root"
        empty_home = base / "empty-home"
        empty_root.mkdir()
        empty_home.mkdir()

        startup = [
            startup_samples(
                packaged_binary,
                empty_root,
                empty_home,
                modern=modern,
                count=arguments.startup_samples,
            )
            for modern in (True, False)
        ]
        corpora = [measure_corpus(packaged_binary, base, count) for count in arguments.file_counts]
        largest = max(corpora, key=lambda corpus: corpus["files"])
        largest_root = base / f"corpus-{largest['files']}"
        largest_home = base / f"home-{largest['files']}"
        tools = tool_measurements(
            packaged_binary,
            largest_root,
            largest_home,
            largest["files"],
            arguments.lookup_samples,
        )
        edit_corpus = next(
            (corpus for corpus in corpora if corpus["files"] == 10_000), None
        )
        watch_edit = (
            watch_edit_measurements(
                packaged_binary,
                base / "corpus-10000",
                base / "home-10000",
                arguments.watch_edit_samples,
            )
            if edit_corpus is not None
            else None
        )
        doctor = timed_json(
            packaged_binary,
            empty_home,
            ["doctor", "--root", str(empty_root), "--json"],
        )["result"]
        evidence = {
            "schema_version": 1,
            "recorded_at_unix_seconds": int(time.time()),
            "binary": {
                "source_path": binary_label,
                "measurement_path": "temporary extracted package/codeatlas",
                "sha256": sha256(packaged_binary),
                "size_bytes": packaged_binary.stat().st_size,
                "version": command_output([str(packaged_binary), "--version"]),
                "doctor": doctor,
                "dynamic_dependencies": command_output(["otool", "-L", str(packaged_binary)]),
            },
            "host": {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "processor": platform.processor(),
                "hardware": hardware_summary(),
                "filesystem": command_output(["df", "-h", tempfile.gettempdir()]),
                "power_mode": "not controlled",
                "background_load": "not controlled",
            },
            "cache_policy": {
                "startup": "process-cold; filesystem/page cache uncontrolled",
                "full_index": "new generated source and empty database",
                "no_op_and_edit": "warm database and OS cache uncontrolled",
                "lookup": "warm persistent MCP process and active SQLite generation",
            },
            "startup": startup,
            "corpora": corpora,
            "tools": tools,
            "watch_single_file_edit": watch_edit,
            "limitations": [
                "Synthetic Rust corpora are deterministic capacity baselines, not production-repository accuracy evidence.",
                "Wall-clock and RSS samples are single-host observations with uncontrolled OS cache and background load.",
                "Peak RSS is sampled from ps at roughly 5 ms intervals and can miss very short-lived maxima.",
            ],
        }
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
