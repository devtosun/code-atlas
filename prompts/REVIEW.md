# Independent review of one completed phase

$ca-security-review $ca-release-validation

Read AGENTS.md, project state and the phase report identified by the user. Inspect
the implementation and actual tests independently. Check every promised acceptance
gate, boundaries, false certainty in graph outputs, source privacy, stale generations,
stdout protocol purity, cancellation and configuration safety relevant to this phase.

Do not edit production code in this review. Run safe read-only/temporary-fixture
checks. Report findings by severity with file/range, reproduction, expected behavior
and suggested fix. Separate demonstrated bugs from untested risks. Do not approve
merely because cargo test passed. Name missing evidence and whether the phase can
legitimately be considered complete.
