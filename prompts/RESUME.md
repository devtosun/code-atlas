# Resume the current phase without scope drift

$ca-phase-driver

Read AGENTS.md and docs/PROJECT_STATE.md. Inspect the current worktree and the last
phase report. Identify the unfinished requested phase, its acceptance gates and
actual failures. Read only its references and relevant skills. Continue that phase
from existing code, preserving unrelated changes. Do not restart the project, skip
gates, accept snapshots blindly or execute the next phase automatically.

Run focused regression checks, then applicable workspace gates. Update the phase
report/state with commands and exit codes. Report any blocker honestly and give the
exact next actionable step. Do not modify user config, commit/push or publish.
