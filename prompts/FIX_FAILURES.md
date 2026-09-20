# Repair failed gates without weakening the contract

$ca-phase-driver

Read AGENTS.md, project state, current phase prompt and latest failed test logs.
Reproduce the failures using the existing fixture inputs. Explain the root cause,
then make the smallest coherent implementation/test-fixture correction that preserves
the acceptance criteria. Add a regression test when the bug was previously untested.

Do not delete tests, lower assertions, replace Tree-sitter with regex, mark language
support complete from empty output, increase timeouts to hide blocked startup or
fabricate an external dependency version. A snapshot change requires comparison to
original source and independently justified expected behavior.

Rerun the focused tests and applicable workspace gates, update evidence/state, and
stop. Preserve unrelated changes and do not advance to another phase automatically.
