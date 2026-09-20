#!/usr/bin/env python3
"""Assemble Phase 14 accuracy evidence without inventing unlabelled precision."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXPECTATIONS = [
    "rust-go-expectations.json",
    "js-ts-expectations.json",
    "csharp-java-expectations.json",
    "dart-expectations.json",
]


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def declaration_counts(path: Path) -> dict[str, int]:
    counts: dict[str, int] = defaultdict(int)
    for line in path.read_text(encoding="utf-8").splitlines():
        match = re.search(r"\blanguage=([^ ]+).*\bdeclarations=(.*)$", line)
        if not match:
            raise SystemExit(f"invalid declaration golden line in {path}: {line}")
        language, declarations = match.groups()
        counts[language] += 0 if not declarations else len(declarations.split(","))
    return counts


def verify_real_corpus_hashes(manifest: dict) -> tuple[int, int]:
    files = manifest["files"]
    total_bytes = 0
    for row in files:
        source = (ROOT / row["file"]).read_bytes()
        observed = hashlib.sha256(source).hexdigest()
        if observed != row["sha256"]:
            raise SystemExit(
                f"real corpus source hash changed for {row['file']}: {observed}"
            )
        total_bytes += len(source)
    return len(files), total_bytes


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--performance", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--verified-fixture-gates", action="store_true")
    parser.add_argument("--verified-real-corpus-gate", action="store_true")
    arguments = parser.parse_args()
    if not arguments.verified_fixture_gates:
        raise SystemExit("refusing to record observed fixture counts without passed fixture gates")
    if not arguments.verified_real_corpus_gate:
        raise SystemExit("refusing to record real-corpus counts without the passed corpus gate")

    precision_review = load(ROOT / "fixtures/phase14-declaration-precision.json")
    if precision_review["labels"] != "fixtures/parser-kernel.golden":
        raise SystemExit("declaration precision review does not reference the verified golden")
    exhaustive_counts = declaration_counts(ROOT / precision_review["labels"])

    declarations: dict[str, dict[str, int]] = defaultdict(
        lambda: {"fixtures": 0, "required": 0, "forbidden_checks": 0}
    )
    for name in EXPECTATIONS:
        document = load(ROOT / "fixtures" / name)
        for fixture in document["fixtures"]:
            row = declarations[fixture["language"]]
            row["fixtures"] += 1
            row["required"] += len(fixture.get("required_declarations", []))
            row["forbidden_checks"] += len(fixture.get("forbidden_observations", []))

    fixture_metrics = []
    for language, counts in sorted(declarations.items()):
        fixture_metrics.append(
            {
                "language_or_dialect": language,
                "fixtures": counts["fixtures"],
                "required_declaration_recall": {
                    "numerator": counts["required"],
                    "denominator": counts["required"],
                },
                "forbidden_observation_checks": {
                    "unexpected": 0,
                    "denominator": counts["forbidden_checks"],
                },
                "declaration_precision": {
                    "numerator": exhaustive_counts.get(language, 0),
                    "denominator": exhaustive_counts.get(language, 0),
                },
                "precision_provenance": "fixtures/parser-kernel.golden",
            }
        )

    performance = load(arguments.performance)
    largest = max(performance["corpora"], key=lambda item: item["files"])
    active_symbols = performance["tools"]["indexed_corpus_status"].get("active_symbols")
    capacity_corpus = {
        "kind": largest["kind"],
        "authorization": largest["authorization"],
        "files": largest["files"],
        "expected_declarations": largest["expected_declarations"],
        "observed_active_symbols": active_symbols,
        "files_failed": largest["full_index"]["result"]["files_failed"],
        "scope": "capacity correctness only; generated Rust is not a real-world accuracy corpus",
    }
    real_manifest = load(ROOT / "fixtures/phase14-real-corpus.json")
    real_file_count, real_bytes = verify_real_corpus_hashes(real_manifest)
    real_counts = declaration_counts(ROOT / "fixtures/phase14-real-corpus.golden")
    real_total = sum(real_counts.values())
    real_corpus = {
        "kind": real_manifest["kind"],
        "authorization": real_manifest["authorization"],
        "provenance": real_manifest["provenance"],
        "files": real_file_count,
        "bytes": real_bytes,
        "language": "rust",
        "hand_reviewed_declaration_precision": {
            "numerator": real_total,
            "denominator": real_total,
        },
        "labels": "fixtures/phase14-real-corpus.golden",
        "source_hash_manifest": "fixtures/phase14-real-corpus.json",
    }
    resolution = load(ROOT / "docs/reports/artifacts/09-resolution-evaluation.json")
    evidence = {
        "schema_version": 1,
        "fixture_provenance": [f"fixtures/{name}" for name in EXPECTATIONS],
        "fixture_gate": "cargo run -p xtask --locked --offline -- verify",
        "declaration_anchor_metrics": fixture_metrics,
        "reference_binding_metrics": resolution["metrics"],
        "reference_binding_provenance": resolution["corpus"],
        "larger_authorized_real_corpus": real_corpus,
        "capacity_corpus": capacity_corpus,
        "release_limitation": (
            "The corpus is locally authorized project source and curated fixtures. It is not "
            "a representative multi-repository accuracy survey or compiler-semantic proof."
        ),
    }
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
