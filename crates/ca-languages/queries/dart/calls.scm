; Optional `new` means many Dart constructor-shaped invocations are call_expression
; nodes; the adapter preserves constructor-like syntax without asserting a target.
(call_expression) @call.invocation.node
(constructor_invocation) @call.explicit_constructor.node
