---
name: ca-phase-driver
description: "Execute one numbered CodeAtlas implementation phase with prerequisite checks, evidence and a handoff; use for phase execution or resumption, not unrelated coding."
---

# Phase Driver


## Inputs
A requested phase file, the current worktree, and docs/PROJECT_STATE.md. The numbered
prompt is the task; this skill is the execution discipline, not permission to run all phases.

## Workflow
Read AGENTS.md and the phase's required documents. Inspect existing implementation,
uncommitted changes and previous report. Confirm prerequisite gates with actual
artifacts; do not trust a completed label without evidence. Write a small execution
plan, then implement one coherent vertical slice at a time with focused tests.

When a source/API assumption is uncertain, inspect pinned upstream docs/source or
run a tiny spike. Record the result. Missing toolchain/network/permission is a real
blocker: do not fabricate a lockfile, substitute regex for parsing, or skip a required
language to make the phase green. Continue independent safe work and report what remains.

Use only the declared scope. Do not turn an optional extension into a required
runtime dependency. Do not modify user configuration, commit, push or publish unless
authorized. Preserve unrelated code and avoid destructive resets.

## Completion
Run phase-specific checks and available standard gates. Fill the phase report from
docs/templates/PHASE_REPORT.md with exact commands/exit codes and observed output.
Update project state and the test matrix. List unsupported syntax and unexecuted
platforms explicitly. Stop after the phase and name the next prompt.

## Read when needed
Repository paths: docs/PROJECT_STATE.md, docs/TEST_MATRIX.md,
docs/templates/PHASE_REPORT.md, docs/templates/ADR.md.
