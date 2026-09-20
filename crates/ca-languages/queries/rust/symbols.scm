; Phase 04 Rust declaration and lexical-scope observations.
(source_file) @scope.file.node
(declaration_list) @scope.declaration_list.node
(block) @scope.block.node
(closure_expression) @scope.closure.node
(function_item) @scope.function.node
(function_signature_item) @scope.function.node

(mod_item) @declaration.module.node
(struct_item) @declaration.struct.node
(enum_item) @declaration.enum.node
(union_item) @declaration.union.node
(trait_item) @declaration.trait.node
(impl_item) @declaration.impl.node
(type_item) @declaration.type_alias.node
(function_item) @declaration.function.node
(function_signature_item) @declaration.function.node
(const_item) @declaration.const.node
(static_item) @declaration.static.node
(macro_definition) @declaration.macro.node
(field_declaration) @declaration.field.node
(enum_variant) @declaration.enum_variant.node
(let_declaration) @declaration.local.node
(parameter) @declaration.parameter.node
(self_parameter) @declaration.self_parameter.node
(closure_parameters) @declaration.closure_parameter.node
(type_parameter) @declaration.type_parameter.node
(const_parameter) @declaration.const_parameter.node

((attribute_item) @condition.cfg.node
  (#match? @condition.cfg.node "^#\\!?\\[cfg(_attr)?\\b"))
((inner_attribute_item) @condition.cfg.node
  (#match? @condition.cfg.node "^#\\!?\\[cfg(_attr)?\\b"))
