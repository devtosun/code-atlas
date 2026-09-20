; Phase 04 Go declaration and lexical-scope observations.
(source_file) @scope.file.node
(block) @scope.block.node
(func_literal) @scope.closure.node
(function_declaration) @scope.function.node
(method_declaration) @scope.method.node
(method_elem) @scope.interface_method.node

(type_spec) @declaration.type.node
(type_alias) @declaration.type_alias.node
(function_declaration) @declaration.function.node
(method_declaration) @declaration.method.node
(method_elem) @declaration.interface_method.node
(const_spec) @declaration.const.node
(var_spec) @declaration.variable.node
(short_var_declaration) @declaration.local.node
(parameter_declaration) @declaration.parameter.node
(variadic_parameter_declaration) @declaration.parameter.node
(type_parameter_declaration) @declaration.type_parameter.node
(field_declaration) @declaration.field.node

((comment) @condition.build_tag.node
  (#match? @condition.build_tag.node "^//(go:build| \\+build)\\b"))
