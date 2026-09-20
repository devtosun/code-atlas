---
name: ca-treesitter-language
description: "Add, upgrade or debug a required Tree-sitter language adapter, grammar binding, .scm capture query or extraction fixture; do not claim compiler-level semantics from syntax alone."
---

# Tree-sitter Languages


## Inputs
Language/dialect, pinned runtime and grammar, node-types.json, fixtures, and
existing capture contracts. Read docs/LANGUAGE_SUPPORT.md and GRAMMAR_MATRIX.md.

## Workflow
Verify the grammar package provenance/license and ABI with the selected runtime.
Inspect actual node types and small parse S-expressions before writing queries.
Compile every .scm query against that grammar. Embed trusted queries in the binary.
Do not guess node names from another language or silently skip failed captures.

Build declaration/container/scope extraction first, then imports/exports and
reference/call-site observations. Recognize call sites even when binding is unknown.
Keep source spelling, UTF-8 byte ranges and file hash. Filter declaration names,
comments and strings from reference occurrences. Treat JSX/TSX as explicit test
fixtures and language capabilities; use the separate TSX grammar.

Test modern constructs and incomplete source. If a construct is unparseable, emit
coverage/diagnostic information and document the limitation. Do not replace Dart
with regex or accept an empty language module. Rust macros are not expanded; C#
partial declarations/overloads and interface dispatch require explicit identities.

## Golden test discipline
Use hand-authored expected names, kinds, containers, call sites and negatives.
Review snapshot diffs against original fixture bytes. Include same-name shadowing,
Unicode/CRLF, syntax errors, deeply nested input, budgets and cancellation. Changing
a grammar/query fingerprint invalidates corresponding stored versions.

## Output
Working adapter + queries + fixtures + accurate capability matrix + exact test
commands. Mark full semantic resolution as unavailable unless a provider proves it.
