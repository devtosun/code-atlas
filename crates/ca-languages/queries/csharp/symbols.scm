; Phase 06 C# declaration, lexical-scope and preprocessor observations.
(compilation_unit) @scope.file.node
(namespace_declaration) @scope.namespace.node
(file_scoped_namespace_declaration) @scope.namespace.node
(declaration_list) @scope.declaration_list.node
(block) @scope.block.node
(method_declaration) @scope.method.node
(constructor_declaration) @scope.constructor.node
(local_function_statement) @scope.local_function.node
(accessor_declaration) @scope.accessor.node

(namespace_declaration) @declaration.namespace.node
(file_scoped_namespace_declaration) @declaration.file_namespace.node
(class_declaration) @declaration.class.node
(struct_declaration) @declaration.struct.node
(interface_declaration) @declaration.interface.node
(enum_declaration) @declaration.enum.node
(record_declaration) @declaration.record.node
(method_declaration) @declaration.method.node
(constructor_declaration) @declaration.constructor.node
(local_function_statement) @declaration.local_function.node
(property_declaration) @declaration.property.node
(accessor_declaration) @declaration.accessor.node
(field_declaration) @declaration.field.node
(enum_member_declaration) @declaration.enum_member.node
(local_declaration_statement (variable_declaration) @declaration.local.node)
(parameter) @declaration.parameter.node
(type_parameter) @declaration.type_parameter.node

(preproc_if condition: (_) @condition.preprocessor.node)
(preproc_elif condition: (_) @condition.preprocessor.node)
(preproc_if_in_attribute_list condition: (_) @condition.preprocessor.node)
