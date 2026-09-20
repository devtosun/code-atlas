; Dart 0.2.0 declarations and lexical scopes. Capture names are consumed by
; DartAdapter; compiler/analyzer identities are intentionally not inferred.
(source_file) @scope.file.node

[
  (class_declaration)
  (mixin_declaration)
  (enum_declaration)
  (extension_declaration)
  (extension_type_declaration)
] @scope.type.node

[
  (function_declaration)
  (getter_declaration)
  (setter_declaration)
  (external_function_declaration)
  (external_getter_declaration)
  (external_setter_declaration)
] @scope.function.node

(method_declaration) @scope.method.node
(function_expression) @scope.closure.node
(block) @scope.block.node

(class_declaration name: (identifier) @declaration.class.name)
(mixin_declaration name: (identifier) @declaration.mixin.name)
(enum_declaration name: (identifier) @declaration.enum.name)
(extension_declaration name: (identifier) @declaration.extension.name)
(extension_type_declaration
  name: (extension_type_name (identifier) @declaration.extension_type.name))
(extension_type_declaration name: (identifier) @declaration.extension_type.name)
(extension_type_representation name: (identifier) @declaration.representation_field.name)
(type_alias (type_identifier) @declaration.type_alias.name)
(enum_constant name: (identifier) @declaration.enum_constant.name)

(function_declaration
  signature: (function_signature name: (identifier) @declaration.function.name))
(getter_declaration
  signature: (getter_signature name: (identifier) @declaration.getter.name))
(setter_declaration
  signature: (setter_signature name: (identifier) @declaration.setter.name))
(external_function_declaration
  signature: (function_signature name: (identifier) @declaration.function.name))
(external_getter_declaration
  signature: (getter_signature name: (identifier) @declaration.getter.name))
(external_setter_declaration
  signature: (setter_signature name: (identifier) @declaration.setter.name))

(method_declaration
  signature: (method_signature
    (function_signature name: (identifier) @declaration.method.name)))
(method_declaration
  signature: (method_signature
    (getter_signature name: (identifier) @declaration.getter.name)))
(method_declaration
  signature: (method_signature
    (setter_signature name: (identifier) @declaration.setter.name)))
(method_declaration
  signature: (method_signature
    (operator_signature) @declaration.operator.node))

(declaration (constructor_signature) @declaration.constructor.node)
(declaration (constant_constructor_signature) @declaration.constructor.node)
(declaration (factory_constructor_signature) @declaration.factory_constructor.node)
(declaration
  (redirecting_factory_constructor_signature) @declaration.redirecting_factory_constructor.node)
(method_declaration
  signature: (method_signature
    (constructor_signature) @declaration.constructor.node))
(method_declaration
  signature: (method_signature
    (factory_constructor_signature) @declaration.factory_constructor.node))

(initialized_identifier name: (identifier) @declaration.variable.name)
(initialized_variable_definition name: (identifier) @declaration.variable.name)
(static_final_declaration name: (identifier) @declaration.variable.name)
(formal_parameter) @declaration.parameter.node
(type_parameter name: (type_identifier) @declaration.type_parameter.name)
(variable_pattern name: (identifier) @declaration.pattern_variable.name)
