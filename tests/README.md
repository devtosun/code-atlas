# Acceptance design

`acceptance-scenarios.json` is a traceable checklist, not an executable test suite.
All cases are NOT_EXECUTED. Convert them into Rust unit/integration, protocol-client,
process-lifecycle and native-platform tests in their assigned phases. Keep their IDs
in test names or report mappings. A passing static kit validator does not pass these
application scenarios and does not prove any language/parser/platform capability.

New regressions need new focused fixtures and scenario IDs. Do not replace meaningful
assertions with snapshot acceptance just to close a phase.
