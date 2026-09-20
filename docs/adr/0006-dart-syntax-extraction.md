# ADR-0006: Dart syntax extraction boundaries

Status: accepted
Date: 2026-09-19

## Context

Phase 07 needs useful Dart structure, including Flutter-style source, without
installing or invoking a Dart/Flutter SDK, analyzer, package resolver, pub,
build_runner or generated-code pipeline. Dart syntax permits optional `new`, named
and redirecting constructors, import prefixes, parts, patterns and callable values.
Syntax alone cannot prove that a capitalized invocation is a constructor, bind a
package URI, merge a library's parts, infer a receiver type or construct a semantic
Flutter widget tree.

The pinned `nielsenko/tree-sitter-dart` 0.2.0 grammar parses the reviewed records,
patterns, sealed classes, extension types and switch expressions. Direct node-type
and parse-tree inspection also established a precise limitation: non-ASCII Dart
identifiers produce `ERROR` nodes with this grammar.

## Decision

- Compile separate symbols/scopes, imports, references and calls query assets
  against the pinned Dart grammar. Extraction operates only on the supplied
  immutable UTF-8 bytes.
- Preserve library, import, export, part and part-of URI/name ranges and aliases as
  syntax relationships. Do not resolve package URIs, prefixes or part ownership.
- Give ordinary, named, const, factory and redirecting constructors distinct
  structural declaration identities. Retain leaf name, owner, kind and signature;
  do not manufacture analyzer symbol keys.
- Preserve getter and setter kinds separately and retain generic, async/await,
  parameter, pattern-binding and lexical-scope evidence.
- Mark capitalized invocations as `constructor_like`, but leave every call
  unresolved with no target ID. Store receiver expression source and named-argument
  evidence without inferring types.
- Treat nested Flutter-style invocations and callbacks as ordinary Dart syntax.
  Do not infer framework registration, routes, lifecycle or a semantic widget tree.
- Keep recovered malformed facts where captured, with visible syntax diagnostics.
  Report the pinned grammar's non-ASCII identifier failure explicitly; UTF-8 in
  comments and strings and CRLF byte positions remain supported and tested.
- Never execute indexed code or invoke a Dart/Flutter SDK, analyzer, pub,
  build_runner, package configuration or generated-code process.

## Consequences

The Dart adapter provides bounded declarations, lexical scopes, library
relationships, references and call sites under the same syntax-only contract as
the other required languages. Consumers can distinguish constructor/accessor
forms and inspect Flutter-style nesting without mistaking observations for analyzer
bindings or runtime behavior.

Cross-file part/library merging, package and prefix resolution, receiver typing,
call-target binding and Flutter semantics remain later resolver work or a separately
scoped semantic provider. Replacing or patching the Dart grammar requires new
provenance review, query compilation, fingerprints and fixture evidence; the
current non-ASCII identifier limitation cannot be presented as supported.
