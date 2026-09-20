# ADR-0005: C# and Java syntax extraction boundaries

Status: accepted
Date: 2026-09-19

## Context

Phase 06 needs useful C# and Java structure without installing or invoking .NET,
the JVM, compilers, build tools, project evaluators, dependency restoration,
annotation processors or language servers. Both languages have overloads and
dynamic dispatch; C# additionally has partial types, extension methods and
preprocessor branches. Syntax alone cannot select overloads, merge partial types,
identify a concrete interface/virtual receiver, or infer dependency injection.

## Decision

- Compile separate symbols/scopes, imports, references and calls query assets for
  each pinned grammar. All extraction runs over the supplied immutable UTF-8 bytes.
- Keep every C# partial declaration as its own path-sensitive declaration ID.
  Record a namespace/container/name `partial_group_hint` only as evidence and add
  `partial_group_not_semantically_confirmed`; do not synthesize a merged type.
- Preserve callable signatures and structural declaration IDs so overloads and
  same-name methods are not collapsed to a simple name.
- Store member-call receiver expression source in the existing receiver field,
  label it `receiver_expression_source`, and explicitly state that no receiver type
  was inferred. Every call remains unresolved with no target ID.
- Mark C# extension declarations as syntax and keep extension dispatch unresolved.
  Mark interface members as syntax whose implementation is not inferred.
- Record C# preprocessor condition syntax without selecting an active branch.
- Treat C#/Java attributes and annotations as references whose behavior and
  arguments are not evaluated. Treat Java method references as references, not
  executed call edges.
- Keep malformed-tree facts when captured, but diagnostics state that recovered
  `ERROR` subtrees are not fully understood.

## Consequences

The adapters provide bounded declarations, scopes, imports, references, calls and
condition evidence without a .NET/JVM runtime dependency. Consumers can inspect
partial/overload/nesting evidence without mistaking it for compiler identity or
runtime dispatch. Cross-file grouping, overload selection, inheritance binding,
classpath/project evaluation and semantic call resolution remain later work or an
optional semantic-provider concern.
