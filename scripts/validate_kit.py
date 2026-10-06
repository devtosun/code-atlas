#!/usr/bin/env python3
"""Validate repository structure; does not execute the Rust compatibility spike.
Requires Python 3.11+; uses only the standard library. Run from any directory.
"""
from __future__ import annotations
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]

def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)

def load_json(path: str) -> dict:
    return json.loads((ROOT / path).read_text(encoding="utf-8"))

def existing_relative(path: str) -> Path:
    resolved = (ROOT / path).resolve()
    require(resolved.is_relative_to(ROOT), f"Path escapes kit: {path}")
    require(resolved.is_file(), f"Missing file: {path}")
    return resolved

def run() -> list[str]:
    messages: list[str] = []
    manifest = load_json("config/phase-manifest.json")
    phases = manifest["phases"]
    require([p["id"] for p in phases] == [f"{i:02d}" for i in range(18)], "Phase IDs must be 00..17")
    require(sum(not p["optional"] for p in phases) == 16, "Expected 16 required phases")
    for p in phases:
        body = existing_relative(p["file"]).read_text(encoding="utf-8")
        require("Acceptance gates" in body, f"No gates in {p['file']}")
        require("Execution and handoff contract" in body, f"No handoff in {p['file']}")
        require(f"docs/reports/{Path(p['file']).name}" in body, f"Missing phase report path in {p['file']}")
        if p["prerequisite"] is not None:
            require(int(p["prerequisite"]) < int(p["id"]), "Invalid prerequisite order")
        for ref in p["references"]:
            existing_relative(ref)
        for skill in p["skills"]:
            existing_relative(f".agents/skills/{skill}/SKILL.md")
            require(f"${skill}" in body, f"Skill invocation missing: {skill}")
    messages.append("PASS: 18 ordered phase prompts; 16 required / 2 optional; referenced documents and skills exist.")
    skill_paths = sorted((ROOT / ".agents/skills").glob("*/SKILL.md"))
    require(len(skill_paths) == 8, "Expected 8 skills")
    for path in skill_paths:
        body = path.read_text(encoding="utf-8")
        require(body.startswith("---\n"), f"No frontmatter: {path.name}")
        _, front, _ = body.split("---", 2)
        # These authored frontmatters intentionally use flat name/description fields.
        # This is not a general YAML parser or a validation of Codex runtime behavior.
        match = re.search(r"(?m)^name: ([-a-z0-9]+)$", front)
        require(match is not None and match.group(1) == path.parent.name, "Skill name mismatch")
        require(re.search(r'(?m)^description: ".+"$', front) is not None, "Missing description")
        metadata = (path.parent / "agents/openai.yaml").read_text(encoding="utf-8")
        require(f"${path.parent.name}" in metadata, "Metadata default_prompt missing skill invocation")
        require("allow_implicit_invocation: true" in metadata, "Missing skill policy")
    messages.append("PASS: 8 SKILL.md frontmatters and corresponding metadata present (authored flat-field checks).")
    for path in sorted((ROOT / "config").glob("*.toml")):
        with path.open("rb") as handle:
            data = tomllib.load(handle)
        if path.name.startswith("codex."):
            server = data["mcp_servers"]["codeatlas"]
            require(server["required"] is False, "Example must not make MCP mandatory")
            require(server["args"][0] == "serve" and "--root" in server["args"], "Missing explicit serve root")
        else:
            require(data["index"]["auto_index"] is False, "Unexpected auto-index")
            require(data["index"]["watch"] is False, "Unexpected auto-watch")
            require(data["memory"]["allow_writes"] is False, "Unexpected memory write grant")
            require(data["privacy"]["follow_symlinks"] is False, "Unexpected symlink following")
    messages.append("PASS: 3 TOML examples parse; startup, memory-write and symlink defaults are opt-in.")
    fixtures = load_json("fixtures/expectations.json")["fixtures"]
    require(len(fixtures) == 24, "Expected 24 original seed fixtures")
    require(len({f["id"] for f in fixtures}) == len(fixtures), "Duplicate fixture IDs")
    expected_languages = {"rust", "go", "java", "csharp", "dart", "javascript", "typescript", "jsx", "tsx"}
    require({f["language"] for f in fixtures} == expected_languages, "Missing language/dialect seeds")
    for fixture in fixtures:
        existing_relative(fixture["file"])
        require(fixture["execution_status"] == "PARSER_KERNEL_PASS", "Fixture execution evidence is stale")
        require(bool(fixture["required_declaration_names"]), "Fixture lacks hand-authored expected names")
    require(b"\r\n" in (ROOT / "fixtures/unicode/crlf.ts").read_bytes(), "Lost CRLF bytes")
    messages.append("PASS: 24 fixture paths/expectations; 7 languages plus JSX/TSX; CRLF bytes preserved.")
    scenarios = load_json("tests/acceptance-scenarios.json")["scenarios"]
    require(len(scenarios) == 43 and len({s["id"] for s in scenarios}) == 43, "Scenario count/uniqueness")
    executed_scenarios = {
        "P01": "PASS",
        "P02": "PASS",
        "P03": "PASS",
        "P04": "PASS",
        "P05": "PASS",
        "P06": "PARTIAL",
        "P07": "PARTIAL",
        "P08": "PASS",
        "P09": "PASS",
        "P10": "PASS",
        "P11": "PASS",
        "P12": "PASS",
        "P13": "PASS",
        "P14": "PASS",
        "P15": "PASS",
        "P16": "PASS",
        "P17": "PASS",
        "P18": "PASS",
        "P19": "PASS",
        "P20": "PASS",
        "P21": "PASS",
        "P22": "PASS",
        "P23": "PASS",
        "P24": "PASS",
        "P25": "PASS",
        "P26": "PASS",
        "P27": "PASS",
        "P28": "PASS",
        "P29": "PASS",
        "P30": "PASS",
        "P31": "PASS",
        "P32": "PASS",
        "P33": "PASS",
        "P34": "PASS",
        "P35": "PASS",
        "P36": "PASS",
        "P37": "PASS",
        "P38": "PARTIAL",
        "P39": "PASS",
    }
    for scenario in scenarios:
        require(scenario["phase"] in {p["id"] for p in phases}, "Unknown scenario phase")
        expected_status = executed_scenarios.get(scenario["id"], "NOT_EXECUTED")
        require(scenario["status"] == expected_status, "Application scenario status lacks phase evidence")
    messages.append(
        "PASS: 43 distinct acceptance scenarios; 36 passes, 3 partial native/API scenarios "
        "and 4 explicitly NOT_EXECUTED."
    )
    md_paths = sorted(ROOT.rglob("*.md"))
    for path in md_paths:
        text = path.read_text(encoding="utf-8")
        require(bool(text.strip()), f"Empty Markdown: {path}")
        # Validate ordinary local Markdown links. External URLs/fragments are not fetched.
        for target in re.findall(r"(?<!!)\[[^\]]+\]\(([^)]+)\)", text):
            if "://" in target or target.startswith("#"):
                continue
            link = target.split("#", 1)[0]
            require((path.parent / link).is_file(), f"Broken local link {target} in {path}")
    project_state = (ROOT / "docs/PROJECT_STATE.md").read_text(encoding="utf-8")
    require("| 00 | completed |" in project_state, "Phase 00 completion state missing")
    require("| 01 | completed |" in project_state, "Phase 01 completion state missing")
    require("| 02 | completed |" in project_state, "Phase 02 completion state missing")
    for phase in range(14):
        require(f"| {phase:02d} | completed |" in project_state, f"Phase {phase:02d} completion state missing")
    require("| 14 | completed |" in project_state, "Phase 14 completion state missing")
    require("| 15 | completed |" in project_state, "Phase 15 completion state missing")
    require(
        "prompts/14-linux-windows-native-validation.md" in project_state,
        "Deferred native validation prompt missing",
    )
    dependency_lock = load_json("config/dependency-lock.json")
    require(dependency_lock["phase"] == "15-release-and-codex", "Wrong dependency lock phase")
    require("production_phase_14" in dependency_lock, "Missing Phase 14 production dependency record")
    require("production_phase_15" in dependency_lock, "Missing Phase 15 production dependency record")
    require(dependency_lock["sqlite"]["fts5_transaction"] == "pass", "Missing SQLite evidence")
    existing_relative("spikes/compatibility/Cargo.lock")
    existing_relative("docs/reports/00-compatibility-spike.md")
    messages.append(f"PASS: {len(md_paths)} nonempty UTF-8 Markdown documents and ordinary local links checked.")
    return messages

if __name__ == "__main__":
    try:
        for message in run():
            print(message)
        print("RESULT: static repository checks passed. This validator did not rerun the Rust compatibility spike.")
    except (ValueError, OSError, KeyError, TypeError, json.JSONDecodeError, tomllib.TOMLDecodeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        sys.exit(1)
