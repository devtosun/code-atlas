; Broad identifier captures are filtered by DartAdapter using declaration/import
; ranges and syntax roles. Comments and string contents have no identifier nodes.
(identifier) @reference.identifier.name
(type_identifier) @reference.type.name
(annotation name: (_) @reference.annotation.name)
(constructor_tearoff) @reference.constructor_tearoff.node
