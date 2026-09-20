# Repository validation — actual static check results

Validation date: 2026-09-20. Interpreter: Python 3.13.5.
Command: `python3 scripts/validate_kit.py`. Exit code: 0.

```text
PASS: 18 ordered phase prompts; 16 required / 2 optional; referenced documents and skills exist.
PASS: 8 SKILL.md frontmatters and corresponding metadata present (authored flat-field checks).
PASS: 3 TOML examples parse; startup, memory-write and symlink defaults are opt-in.
PASS: 24 fixture paths/expectations; 7 languages plus JSX/TSX; CRLF bytes preserved.
PASS: 43 distinct acceptance scenarios; 36 passes, 3 partial native/API scenarios and 4 explicitly NOT_EXECUTED.
PASS: 79 nonempty UTF-8 Markdown documents and ordinary local links checked.
RESULT: static repository checks passed. This validator did not rerun the Rust compatibility spike.
```

Scope: document structure and local links, phase/skill references, authored flat skill
frontmatter, TOML/JSON parsing, fixture existence and status, safe example defaults,
Phase 14 completion state and Phase 15/deferred-native handoff. This is not a general
YAML/schema conformance test and does not replace native Rust gates.

The final macOS ARM64 Rust evidence is in
`docs/reports/14-hardening-and-evaluation.md`: `xtask verify` runs formatting, strict
clippy, 112 tests with two helper entry points ignored, language goldens and the
reviewed real-corpus gate. Separate artifacts record performance/RSS, accuracy,
process capabilities, dependency scans and bounded fuzz runs. Linux x64 and Windows
x64 MSVC remain not run and unsupported.
