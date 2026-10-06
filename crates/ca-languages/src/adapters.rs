use std::path::Path;

use tree_sitter::Node;

use crate::{
    Diagnostic, DiagnosticSeverity, LanguageError, LanguageId, ResolutionCategory, SourceRange,
    SyntaxObservation, worker::node_range,
};

pub(crate) struct RawCapture<'tree> {
    pub(crate) name: String,
    pub(crate) node: Node<'tree>,
}

#[derive(Default)]
pub(crate) struct ExtractionBuckets {
    pub(crate) declarations: Vec<SyntaxObservation>,
    pub(crate) scopes: Vec<SyntaxObservation>,
    pub(crate) imports: Vec<SyntaxObservation>,
    pub(crate) references: Vec<SyntaxObservation>,
    pub(crate) call_sites: Vec<SyntaxObservation>,
    pub(crate) conditions: Vec<SyntaxObservation>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub(crate) fn extract(
    language: LanguageId,
    relative_path: &Path,
    source: &[u8],
    captures: &[RawCapture<'_>],
) -> Result<ExtractionBuckets, LanguageError> {
    match language {
        LanguageId::Dart => {
            extract_source_language(language, relative_path, source, captures, DartAdapter)
        }
        LanguageId::Rust => {
            extract_source_language(language, relative_path, source, captures, RustAdapter)
        }
        LanguageId::Go => {
            extract_source_language(language, relative_path, source, captures, GoAdapter)
        }
        LanguageId::CSharp => {
            extract_source_language(language, relative_path, source, captures, CSharpAdapter)
        }
        LanguageId::Java => {
            extract_source_language(language, relative_path, source, captures, JavaAdapter)
        }
        LanguageId::JavaScript | LanguageId::Jsx | LanguageId::TypeScript | LanguageId::Tsx => {
            extract_source_language(
                language,
                relative_path,
                source,
                captures,
                EcmaAdapter::new(language),
            )
        }
    }
}

trait SourceAdapter: Copy {
    fn scope_kind(self, node: Node<'_>) -> &'static str;

    fn scope_spelling(self, node: Node<'_>, source: &[u8]) -> Result<String, LanguageError>;

    fn declarations(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError>;

    fn imports(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError>;

    fn call(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Option<SyntaxObservation>, LanguageError>;

    fn exclude_reference(self, capture_kind: &str, node: Node<'_>, source: &[u8]) -> bool;

    fn decorate_reference(
        self,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        observation: &mut SyntaxObservation,
    ) -> Result<(), LanguageError>;

    fn diagnostics(self, conditions: &[SyntaxObservation]) -> Vec<Diagnostic>;
}

fn extract_source_language<A: SourceAdapter>(
    language: LanguageId,
    relative_path: &Path,
    source: &[u8],
    captures: &[RawCapture<'_>],
    adapter: A,
) -> Result<ExtractionBuckets, LanguageError> {
    let mut output = ExtractionBuckets::default();

    for capture in captures {
        let Some((category, kind, _role)) = capture_parts(&capture.name) else {
            continue;
        };
        if category != "scope" {
            continue;
        }
        let syntax_range = node_range(capture.node)?;
        let scope_kind = adapter.scope_kind(capture.node);
        let spelling = adapter.scope_spelling(capture.node, source)?;
        output.scopes.push(new_observation(
            IdentityContext::new(language, relative_path),
            "scope",
            scope_kind,
            spelling,
            syntax_range,
            syntax_range,
            capture.node,
        ));
        let _ = kind;
    }
    if !output.scopes.iter().any(|scope| scope.kind == "file")
        && let Some(first_capture) = captures.first()
    {
        let mut root = first_capture.node;
        while let Some(parent) = root.parent() {
            root = parent;
        }
        let root_range = node_range(root)?;
        output.scopes.push(new_observation(
            IdentityContext::new(language, relative_path),
            "scope",
            "file",
            "<file>".to_owned(),
            root_range,
            root_range,
            root,
        ));
    }
    sort_observations(&mut output.scopes);
    output.scopes.dedup_by(|left, right| left.id == right.id);
    let scope_snapshot = output.scopes.clone();
    for scope in &mut output.scopes {
        scope.scope_id = containing_scope_id(scope.syntax_range, &scope_snapshot, Some(&scope.id));
    }

    for capture in captures {
        let Some((category, kind, _role)) = capture_parts(&capture.name) else {
            continue;
        };
        match category {
            "declaration" => output.declarations.extend(adapter.declarations(
                language,
                relative_path,
                kind,
                capture.node,
                source,
                &output.scopes,
            )?),
            "import" => output.imports.extend(adapter.imports(
                language,
                relative_path,
                kind,
                capture.node,
                source,
                &output.scopes,
            )?),
            "call" => {
                if let Some(call) = adapter.call(
                    language,
                    relative_path,
                    kind,
                    capture.node,
                    source,
                    &output.scopes,
                )? {
                    output.call_sites.push(call);
                }
            }
            "condition" => {
                let range = node_range(capture.node)?;
                let mut observation = new_observation(
                    IdentityContext::new(language, relative_path),
                    "condition",
                    kind,
                    text(capture.node, source)?.trim().to_owned(),
                    range,
                    range,
                    capture.node,
                );
                observation.scope_id = containing_scope_id(range, &output.scopes, None);
                observation.limitations.push(match language {
                    LanguageId::Rust => "active_cfg_not_inferred".to_owned(),
                    LanguageId::Go => "active_build_constraints_not_inferred".to_owned(),
                    _ => "configuration_not_inferred".to_owned(),
                });
                output.conditions.push(observation);
            }
            _ => {}
        }
    }

    sort_observations(&mut output.declarations);
    output
        .declarations
        .dedup_by(|left, right| left.id == right.id);
    sort_observations(&mut output.imports);
    output.imports.dedup_by(|left, right| left.id == right.id);
    sort_observations(&mut output.call_sites);
    output
        .call_sites
        .dedup_by(|left, right| left.id == right.id);
    sort_observations(&mut output.conditions);
    output
        .conditions
        .dedup_by(|left, right| left.id == right.id);

    let excluded_name_ranges: Vec<SourceRange> =
        output.declarations.iter().map(|item| item.range).collect();
    let excluded_syntax_ranges: Vec<SourceRange> = output
        .imports
        .iter()
        .filter(|item| {
            matches!(
                item.kind.as_str(),
                "use"
                    | "package"
                    | "import"
                    | "using"
                    | "esm_import"
                    | "reexport"
                    | "commonjs_require"
                    | "dynamic_import"
            ) || (language == LanguageId::Dart
                && matches!(
                    item.kind.as_str(),
                    "library" | "export" | "part" | "part_of"
                ))
        })
        .chain(output.conditions.iter())
        .map(|item| item.syntax_range)
        .collect();
    for capture in captures {
        let Some((category, kind, _role)) = capture_parts(&capture.name) else {
            continue;
        };
        if category != "reference" || adapter.exclude_reference(kind, capture.node, source) {
            continue;
        }
        let range = node_range(capture.node)?;
        if (matches!(
            language,
            LanguageId::Dart | LanguageId::CSharp | LanguageId::Java
        ) && excluded_name_ranges
            .iter()
            .any(|excluded| excluded.contains(range)))
            || (!matches!(
                language,
                LanguageId::Dart | LanguageId::CSharp | LanguageId::Java
            ) && excluded_name_ranges.contains(&range))
            || excluded_syntax_ranges
                .iter()
                .any(|excluded| excluded.contains(range))
        {
            continue;
        }
        let mut reference = new_observation(
            IdentityContext::new(language, relative_path),
            "reference",
            kind,
            text(capture.node, source)?.to_owned(),
            range,
            range,
            capture.node,
        );
        reference.scope_id = containing_scope_id(range, &output.scopes, None);
        reference.resolution = ResolutionCategory::SyntaxObservation;
        adapter.decorate_reference(kind, capture.node, source, &mut reference)?;
        decorate_member_selector(capture.node, source, &mut reference)?;
        output.references.push(reference);
    }

    output
        .diagnostics
        .extend(adapter.diagnostics(&output.conditions));
    sort_and_dedup(&mut output);
    Ok(output)
}

fn capture_parts(capture: &str) -> Option<(&str, &str, &str)> {
    let mut parts = capture.split('.');
    let category = parts.next()?;
    let kind = parts.next()?;
    let role = parts.next()?;
    (parts.next().is_none()).then_some((category, kind, role))
}

#[derive(Clone, Copy)]
struct IdentityContext<'path> {
    language: LanguageId,
    relative_path: &'path Path,
}

impl<'path> IdentityContext<'path> {
    fn new(language: LanguageId, relative_path: &'path Path) -> Self {
        Self {
            language,
            relative_path,
        }
    }
}

fn new_observation(
    identity: IdentityContext<'_>,
    category: &str,
    kind: &str,
    spelling: String,
    range: SourceRange,
    syntax_range: SourceRange,
    identity_node: Node<'_>,
) -> SyntaxObservation {
    let path = identity.relative_path.to_string_lossy();
    let structural_path = structural_path(identity_node);
    let id = stable_id(&[
        identity.language.as_str(),
        path.as_ref(),
        category,
        kind,
        &spelling,
        &structural_path,
    ]);
    SyntaxObservation {
        id,
        kind: kind.to_owned(),
        spelling,
        range,
        syntax_range,
        scope_id: None,
        container: None,
        signature: None,
        receiver_type: None,
        alias: None,
        target_id: None,
        resolution: ResolutionCategory::SyntaxObservation,
        attributes: Vec::new(),
        limitations: Vec::new(),
    }
}

fn stable_id(parts: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

fn structural_path(mut node: Node<'_>) -> String {
    let mut segments = Vec::new();
    while let Some(parent) = node.parent() {
        let mut selected_index = 0_usize;
        let mut cursor = parent.walk();
        for (named_index, child) in parent.named_children(&mut cursor).enumerate() {
            if child == node {
                selected_index = named_index;
                break;
            }
        }
        segments.push(format!("{}:{selected_index}", node.kind()));
        node = parent;
    }
    segments.push(node.kind().to_owned());
    segments.reverse();
    segments.join("/")
}

fn containing_scope_id(
    range: SourceRange,
    scopes: &[SyntaxObservation],
    excluded_id: Option<&str>,
) -> Option<String> {
    scopes
        .iter()
        .filter(|scope| excluded_id != Some(scope.id.as_str()))
        .filter(|scope| scope.syntax_range.contains(range))
        .filter(|scope| {
            excluded_id.is_none() || scope.syntax_range != range || scope.kind == "file"
        })
        .min_by_key(|scope| {
            (
                scope
                    .syntax_range
                    .bytes
                    .end()
                    .saturating_sub(scope.syntax_range.bytes.start()),
                scope.kind == "file",
                scope.id.as_str(),
            )
        })
        .map(|scope| scope.id.clone())
}

fn text<'source>(node: Node<'_>, source: &'source [u8]) -> Result<&'source str, LanguageError> {
    node.utf8_text(source)
        .map_err(|error| LanguageError::InvalidUtf8 {
            valid_up_to: error.valid_up_to(),
        })
}

// Selectors are not bare lexical names. These fields come from the pinned grammars.
fn decorate_member_selector(
    node: Node<'_>,
    source: &[u8],
    observation: &mut SyntaxObservation,
) -> Result<(), LanguageError> {
    let Some(parent) = node.parent() else {
        return Ok(());
    };
    let (selector, receiver) = match parent.kind() {
        "field_expression" => ("field", "value"),
        "selector_expression" => ("field", "operand"),
        "member_access_expression" => ("name", "expression"),
        "field_access" => ("field", "object"),
        "member_expression" | "null_aware_member_expression" => ("property", "object"),
        _ => return Ok(()),
    };
    if parent.child_by_field_name(selector) == Some(node) {
        observation.receiver_type = parent
            .child_by_field_name(receiver)
            .map(|receiver| text(receiver, source).map(str::to_owned))
            .transpose()?;
        observation.attributes.push("member_selector".to_owned());
        observation
            .limitations
            .push("receiver_type_not_inferred".to_owned());
    }
    Ok(())
}

fn signature_before_body(node: Node<'_>, source: &[u8]) -> Result<String, LanguageError> {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte());
    let bytes = source
        .get(node.start_byte()..end)
        .ok_or(LanguageError::CoordinateOverflow)?;
    let signature = std::str::from_utf8(bytes).map_err(|error| LanguageError::InvalidUtf8 {
        valid_up_to: error.valid_up_to(),
    })?;
    Ok(signature.trim().to_owned())
}

fn nearest_ancestor<'tree>(mut node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    while let Some(parent) = node.parent() {
        if kinds.contains(&parent.kind()) {
            return Some(parent);
        }
        node = parent;
    }
    None
}

fn field_nodes<'tree>(node: Node<'tree>, field: &str) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor).collect()
}

fn named_descendants<'tree>(node: Node<'tree>, accepted: &[&str]) -> Vec<Node<'tree>> {
    fn visit<'tree>(node: Node<'tree>, accepted: &[&str], output: &mut Vec<Node<'tree>>) {
        if accepted.contains(&node.kind()) {
            output.push(node);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            visit(child, accepted, output);
        }
    }
    let mut output = Vec::new();
    visit(node, accepted, &mut output);
    output
}

fn set_context(
    observation: &mut SyntaxObservation,
    scopes: &[SyntaxObservation],
    container: Option<String>,
) {
    let own_scope_id = scopes
        .iter()
        .find(|scope| scope.syntax_range == observation.syntax_range)
        .map(|scope| scope.id.as_str());
    observation.scope_id = containing_scope_id(observation.range, scopes, own_scope_id);
    observation.container = container;
}

fn sort_observations(items: &mut [SyntaxObservation]) {
    items.sort_by(|left, right| {
        (
            left.range.bytes.start(),
            left.range.bytes.end(),
            left.kind.as_str(),
            left.spelling.as_str(),
        )
            .cmp(&(
                right.range.bytes.start(),
                right.range.bytes.end(),
                right.kind.as_str(),
                right.spelling.as_str(),
            ))
    });
}

fn sort_and_dedup(output: &mut ExtractionBuckets) {
    for items in [
        &mut output.declarations,
        &mut output.scopes,
        &mut output.imports,
        &mut output.references,
        &mut output.call_sites,
        &mut output.conditions,
    ] {
        sort_observations(items);
        items.dedup_by(|left, right| left.id == right.id);
    }
}

#[derive(Clone, Copy)]
struct DartAdapter;

impl DartAdapter {
    fn syntax_node<'tree>(self, capture_kind: &str, node: Node<'tree>) -> Node<'tree> {
        let kinds: &[&str] = match capture_kind {
            "class" => &["class_declaration"],
            "mixin" => &["mixin_declaration"],
            "enum" => &["enum_declaration"],
            "extension" => &["extension_declaration"],
            "extension_type" => &["extension_type_declaration"],
            "representation_field" => &["extension_type_representation"],
            "type_alias" => &["type_alias"],
            "enum_constant" => &["enum_constant"],
            "function" => &["function_declaration", "external_function_declaration"],
            "getter" => &[
                "method_declaration",
                "getter_declaration",
                "external_getter_declaration",
            ],
            "setter" => &[
                "method_declaration",
                "setter_declaration",
                "external_setter_declaration",
            ],
            "method" | "operator" => &["method_declaration"],
            "constructor" | "factory_constructor" | "redirecting_factory_constructor" => {
                &["method_declaration", "declaration"]
            }
            "variable" => &[
                "initialized_identifier",
                "initialized_variable_definition",
                "static_final_declaration",
            ],
            "parameter" => &["formal_parameter"],
            "type_parameter" => &["type_parameter"],
            "pattern_variable" => &["variable_pattern"],
            _ => &[],
        };
        if kinds.contains(&node.kind()) {
            node
        } else {
            nearest_ancestor(node, kinds).unwrap_or(node)
        }
    }

    fn callable_signature<'tree>(self, node: Node<'tree>) -> Option<Node<'tree>> {
        const SIGNATURES: &[&str] = &[
            "function_signature",
            "getter_signature",
            "setter_signature",
            "operator_signature",
            "constructor_signature",
            "constant_constructor_signature",
            "factory_constructor_signature",
            "redirecting_factory_constructor_signature",
        ];
        if SIGNATURES.contains(&node.kind()) {
            Some(node)
        } else {
            first_named_descendant(node, SIGNATURES)
        }
    }

    fn callable_name_node<'tree>(self, node: Node<'tree>) -> Option<Node<'tree>> {
        let signature = self.callable_signature(node)?;
        if signature.kind() == "operator_signature" {
            return signature.child_by_field_name("operator");
        }
        let names = field_nodes(signature, "name")
            .into_iter()
            .filter(Node::is_named)
            .collect::<Vec<_>>();
        names.last().copied()
    }

    fn type_name_node<'tree>(self, node: Node<'tree>) -> Option<Node<'tree>> {
        let name = node.child_by_field_name("name")?;
        if name.kind() == "extension_type_name" {
            first_named_descendant(name, &["identifier"])
        } else {
            Some(name)
        }
    }

    fn container(self, node: Node<'_>, source: &[u8]) -> Result<Option<String>, LanguageError> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if matches!(
                parent.kind(),
                "function_declaration"
                    | "getter_declaration"
                    | "setter_declaration"
                    | "external_function_declaration"
                    | "external_getter_declaration"
                    | "external_setter_declaration"
                    | "method_declaration"
            ) && let Some(name) = self.callable_name_node(parent)
            {
                return Ok(Some(text(name, source)?.to_owned()));
            }
            if matches!(
                parent.kind(),
                "class_declaration"
                    | "mixin_declaration"
                    | "enum_declaration"
                    | "extension_declaration"
                    | "extension_type_declaration"
            ) && let Some(name) = self.type_name_node(parent)
            {
                return Ok(Some(text(name, source)?.to_owned()));
            }
            current = parent;
        }
        Ok(None)
    }

    fn attached_annotations(
        self,
        node: Node<'_>,
        source: &[u8],
    ) -> Result<Vec<String>, LanguageError> {
        let mut output = Vec::new();
        for annotation in immediate_named_children(node, "annotation") {
            if let Some(name) = annotation.child_by_field_name("name") {
                output.push(text(name, source)?.to_owned());
            }
        }
        Ok(output)
    }

    fn parameter_name_node<'tree>(self, node: Node<'tree>) -> Option<Node<'tree>> {
        node.child_by_field_name("name").or_else(|| {
            first_named_descendant(node, &["constructor_param", "super_formal_parameter"])
                .and_then(|parameter| first_named_descendant(parameter, &["identifier"]))
        })
    }

    fn constructor_name_nodes<'tree>(self, node: Node<'tree>) -> Vec<Node<'tree>> {
        field_nodes(node, "name")
            .into_iter()
            .filter(Node::is_named)
            .filter(|name| name.kind() == "identifier")
            .collect()
    }

    fn declaration_name_node<'tree>(
        self,
        capture_kind: &str,
        node: Node<'tree>,
    ) -> Option<Node<'tree>> {
        match capture_kind {
            "constructor" | "factory_constructor" | "redirecting_factory_constructor" => {
                self.constructor_name_nodes(node).last().copied()
            }
            "operator" => node.child_by_field_name("operator"),
            "parameter" => self.parameter_name_node(node),
            _ => Some(node),
        }
    }

    fn uri_content_node<'tree>(self, node: Node<'tree>) -> Option<Node<'tree>> {
        first_named_descendant(
            node,
            &[
                "template_chars_single_single",
                "template_chars_double_single",
                "template_chars_single",
                "template_chars_double",
                "template_chars_raw_slash",
            ],
        )
    }

    fn import_target_node<'tree>(
        self,
        capture_kind: &str,
        node: Node<'tree>,
    ) -> Option<Node<'tree>> {
        if capture_kind == "library" {
            return first_named_descendant(node, &["dotted_identifier_list"]);
        }
        if capture_kind == "part_of"
            && let Some(name) = first_named_descendant(node, &["dotted_identifier_list"])
        {
            return Some(name);
        }
        self.uri_content_node(node)
    }

    fn unwrap_instantiation<'tree>(self, mut node: Node<'tree>) -> Node<'tree> {
        while node.kind() == "instantiation_expression" {
            let Some(function) = node.child_by_field_name("function") else {
                break;
            };
            node = function;
        }
        node
    }
}

impl SourceAdapter for DartAdapter {
    fn scope_kind(self, node: Node<'_>) -> &'static str {
        match node.kind() {
            "source_file" => "file",
            "class_declaration" => "class",
            "mixin_declaration" => "mixin",
            "enum_declaration" => "enum",
            "extension_declaration" => "extension",
            "extension_type_declaration" => "extension_type",
            "function_declaration" | "external_function_declaration" => "function",
            "getter_declaration" | "external_getter_declaration" => "getter",
            "setter_declaration" | "external_setter_declaration" => "setter",
            "method_declaration" => "method",
            "function_expression" => "closure",
            "block" => "block",
            _ => "scope",
        }
    }

    fn scope_spelling(self, node: Node<'_>, source: &[u8]) -> Result<String, LanguageError> {
        match node.kind() {
            "source_file" => Ok("<file>".to_owned()),
            "function_expression" => Ok("<closure>".to_owned()),
            "block" => Ok("<block>".to_owned()),
            "class_declaration"
            | "mixin_declaration"
            | "enum_declaration"
            | "extension_declaration"
            | "extension_type_declaration" => self
                .type_name_node(node)
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| format!("<{}>", node.kind()))),
            _ => self
                .callable_name_node(node)
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| format!("<{}>", node.kind()))),
        }
    }

    fn declarations(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let syntax_node = self.syntax_node(capture_kind, node);
        let Some(name_node) = self.declaration_name_node(capture_kind, node) else {
            return Ok(Vec::new());
        };
        let mut kind = capture_kind;
        if capture_kind == "variable" {
            kind = if nearest_ancestor(name_node, &["local_variable_declaration"]).is_some() {
                "local"
            } else if nearest_ancestor(name_node, &["class_member"]).is_some() {
                "field"
            } else {
                "variable"
            };
        }
        let range = node_range(name_node)?;
        let spelling = text(name_node, source)?.to_owned();
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "declaration",
            kind,
            spelling,
            range,
            node_range(syntax_node)?,
            name_node,
        );
        set_context(
            &mut observation,
            scopes,
            self.container(syntax_node, source)?,
        );

        if matches!(
            capture_kind,
            "function"
                | "getter"
                | "setter"
                | "method"
                | "operator"
                | "constructor"
                | "factory_constructor"
                | "redirecting_factory_constructor"
        ) {
            observation.signature = Some(signature_before_body(syntax_node, source)?);
        }
        let generic_root = self.callable_signature(syntax_node).unwrap_or(syntax_node);
        if syntax_node.child_by_field_name("type_parameters").is_some()
            || !named_descendants(generic_root, &["type_parameters"]).is_empty()
        {
            observation.attributes.push("generic_syntax".to_owned());
        }
        if syntax_node.child_by_field_name("body").is_some_and(|body| {
            text(body, source).is_ok_and(|body| body.trim_start().starts_with("async"))
        }) {
            observation.attributes.push("async".to_owned());
        }
        let header = signature_before_body(syntax_node, source)?;
        for modifier in ["sealed", "base", "interface", "final"] {
            if header
                .split(|character: char| !character.is_alphanumeric() && character != '_')
                .any(|word| word == modifier)
            {
                observation.attributes.push(format!("modifier:{modifier}"));
            }
        }
        for annotation in self.attached_annotations(syntax_node, source)? {
            observation
                .attributes
                .push(format!("annotation:{annotation}"));
            observation
                .limitations
                .push("annotation_behavior_not_evaluated".to_owned());
        }
        if matches!(
            capture_kind,
            "constructor" | "factory_constructor" | "redirecting_factory_constructor"
        ) {
            let constructor_names = self.constructor_name_nodes(node);
            if let Some(owner) = constructor_names.first() {
                observation
                    .attributes
                    .push(format!("constructor_owner:{}", text(*owner, source)?));
            }
            if constructor_names.len() > 1 {
                observation.attributes.push("named_constructor".to_owned());
            }
            if node.kind() == "constant_constructor_signature" {
                observation.attributes.push("const_constructor".to_owned());
            }
            if capture_kind.contains("factory") {
                observation
                    .attributes
                    .push("factory_constructor".to_owned());
            }
            if capture_kind == "redirecting_factory_constructor" {
                observation
                    .attributes
                    .push("redirecting_constructor".to_owned());
                observation
                    .limitations
                    .push("redirect_target_not_resolved".to_owned());
            }
            observation
                .limitations
                .push("constructor_identity_is_syntax_only".to_owned());
        }
        if capture_kind == "pattern_variable" {
            observation
                .attributes
                .push("pattern_binding_syntax".to_owned());
        }
        Ok(vec![observation])
    }

    fn imports(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let Some(target) = self.import_target_node(capture_kind, node) else {
            return Ok(Vec::new());
        };
        let range = node_range(target)?;
        let spelling = text(target, source)?.to_owned();
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "import",
            capture_kind,
            spelling.clone(),
            range,
            node_range(node)?,
            target,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        if capture_kind == "import"
            && let Some(specification) = first_named_descendant(node, &["import_specification"])
        {
            observation.alias = specification
                .child_by_field_name("alias")
                .map(|alias| text(alias, source).map(str::to_owned))
                .transpose()?;
        }
        for combinator in named_descendants(node, &["combinator"]) {
            let mut cursor = combinator.walk();
            let names = combinator
                .named_children(&mut cursor)
                .filter(|child| child.kind() == "identifier")
                .map(|child| text(child, source))
                .collect::<Result<Vec<_>, _>>()?
                .join(",");
            let kind = if text(combinator, source)?.trim_start().starts_with("show") {
                "show"
            } else {
                "hide"
            };
            observation.attributes.push(format!("{kind}:{names}"));
        }
        if spelling.starts_with("package:") {
            observation
                .attributes
                .push("external_package_uri".to_owned());
        } else if spelling.starts_with("dart:") {
            observation.attributes.push("dart_sdk_uri".to_owned());
        }
        if first_named_descendant(node, &["configuration_uri"]).is_some() {
            observation
                .attributes
                .push("conditional_uri_syntax".to_owned());
            observation
                .limitations
                .push("active_import_configuration_not_inferred".to_owned());
        }
        observation
            .limitations
            .push("package_and_library_resolution_not_performed".to_owned());
        match capture_kind {
            "export" => observation
                .limitations
                .push("export_cycle_guard_required".to_owned()),
            "part" | "part_of" => observation
                .limitations
                .push("part_relationship_not_resolved".to_owned()),
            "library" => observation
                .limitations
                .push("library_identity_not_analyzer_resolved".to_owned()),
            _ => {}
        }
        Ok(vec![observation])
    }

    fn call(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Option<SyntaxObservation>, LanguageError> {
        let (kind, target, receiver) = if capture_kind == "explicit_constructor" {
            let constructor = node.child_by_field_name("constructor");
            let target =
                constructor.or_else(|| field_nodes(node, "type").into_iter().rfind(Node::is_named));
            let Some(target) = target else {
                return Ok(None);
            };
            let receiver = field_nodes(node, "type").into_iter().find(Node::is_named);
            ("constructor", target, receiver)
        } else {
            let Some(function) = node.child_by_field_name("function") else {
                return Ok(None);
            };
            let function = self.unwrap_instantiation(function);
            match function.kind() {
                "member_expression" | "null_aware_member_expression" => {
                    let Some(property) = function.child_by_field_name("property") else {
                        return Ok(None);
                    };
                    ("method", property, function.child_by_field_name("object"))
                }
                _ => {
                    let kind = if text(function, source)?
                        .chars()
                        .next()
                        .is_some_and(char::is_uppercase)
                    {
                        "constructor_like"
                    } else {
                        "function"
                    };
                    (kind, function, None)
                }
            }
        };
        let range = node_range(target)?;
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "call",
            kind,
            text(target, source)?.to_owned(),
            range,
            node_range(node)?,
            node,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        observation.container = self.container(node, source)?;
        observation.resolution = ResolutionCategory::Unresolved;
        if let Some(receiver) = receiver {
            observation.receiver_type = Some(text(receiver, source)?.to_owned());
            observation
                .attributes
                .push("receiver_expression_source".to_owned());
            observation
                .limitations
                .push("receiver_type_or_import_prefix_not_inferred".to_owned());
        }
        if kind == "constructor_like" {
            observation
                .attributes
                .push("capitalized_constructor_like_syntax".to_owned());
            observation
                .limitations
                .push("constructor_or_function_target_not_resolved".to_owned());
            observation
                .limitations
                .push("flutter_widget_identity_and_tree_not_inferred".to_owned());
        } else if kind == "constructor" {
            observation
                .limitations
                .push("constructor_target_not_resolved".to_owned());
        } else if kind == "method" {
            observation
                .limitations
                .push("dynamic_dispatch_target_not_resolved".to_owned());
        } else {
            observation
                .limitations
                .push("call_target_not_resolved".to_owned());
        }
        if nearest_ancestor(node, &["await_expression"]).is_some() {
            observation.attributes.push("awaited".to_owned());
        }
        if node
            .child_by_field_name("arguments")
            .is_some_and(|arguments| !named_descendants(arguments, &["named_argument"]).is_empty())
        {
            observation
                .attributes
                .push("named_arguments_syntax".to_owned());
        }
        Ok(Some(observation))
    }

    fn exclude_reference(self, capture_kind: &str, node: Node<'_>, source: &[u8]) -> bool {
        if text(node, source).is_ok_and(|spelling| spelling == "_") {
            return true;
        }
        if capture_kind != "annotation" && nearest_ancestor(node, &["annotation"]).is_some() {
            return true;
        }
        if capture_kind != "constructor_tearoff"
            && nearest_ancestor(node, &["constructor_tearoff"]).is_some()
        {
            return true;
        }
        nearest_ancestor(node, &["label"])
            .and_then(|label| label.parent())
            .is_some_and(|parent| parent.kind() == "named_argument")
    }

    fn decorate_reference(
        self,
        capture_kind: &str,
        node: Node<'_>,
        _source: &[u8],
        observation: &mut SyntaxObservation,
    ) -> Result<(), LanguageError> {
        match capture_kind {
            "type" => observation.attributes.push("role:type".to_owned()),
            "annotation" => {
                observation.attributes.push("role:annotation".to_owned());
                observation
                    .limitations
                    .push("annotation_target_and_behavior_not_resolved".to_owned());
            }
            "constructor_tearoff" => {
                observation
                    .attributes
                    .push("constructor_tearoff_syntax".to_owned());
                observation
                    .limitations
                    .push("constructor_tearoff_not_a_call_edge".to_owned());
            }
            _ => {
                observation.attributes.push("role:value".to_owned());
                if let Some(member) =
                    nearest_ancestor(node, &["member_expression", "null_aware_member_expression"])
                    && member.child_by_field_name("object") == Some(node)
                {
                    observation
                        .attributes
                        .push("member_receiver_or_import_prefix_syntax".to_owned());
                    observation
                        .limitations
                        .push("import_prefix_vs_value_not_resolved".to_owned());
                }
            }
        }
        Ok(())
    }

    fn diagnostics(self, _conditions: &[SyntaxObservation]) -> Vec<Diagnostic> {
        vec![Diagnostic {
            code: "dart_source_only_limits".to_owned(),
            message: "Dart extraction is syntax-only: no Dart/Flutter SDK, analyzer, pub, build_runner, package configuration, generated code, type inference, dispatch resolution, annotation execution, or semantic widget tree is used; this pinned grammar does not accept non-ASCII identifiers, and recovered ERROR subtrees are not treated as fully understood".to_owned(),
            severity: DiagnosticSeverity::Information,
            range: None,
        }]
    }
}

#[derive(Clone, Copy)]
struct RustAdapter;

impl RustAdapter {
    fn container(self, node: Node<'_>, source: &[u8]) -> Result<Option<String>, LanguageError> {
        let Some(container) = nearest_ancestor(
            node,
            &[
                "impl_item",
                "trait_item",
                "mod_item",
                "struct_item",
                "enum_item",
                "union_item",
            ],
        ) else {
            return Ok(None);
        };
        match container.kind() {
            "impl_item" => Ok(Some(signature_before_body(container, source)?)),
            _ => container
                .child_by_field_name("name")
                .map(|name| text(name, source).map(str::to_owned))
                .transpose(),
        }
    }

    fn declaration_from_name(
        self,
        identity: IdentityContext<'_>,
        kind: &str,
        name_node: Node<'_>,
        syntax_node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<SyntaxObservation, LanguageError> {
        let range = node_range(name_node)?;
        let mut observation = new_observation(
            identity,
            "declaration",
            kind,
            text(name_node, source)?.to_owned(),
            range,
            node_range(syntax_node)?,
            name_node,
        );
        set_context(
            &mut observation,
            scopes,
            self.container(syntax_node, source)?,
        );
        if matches!(kind, "function" | "method" | "trait_method") {
            observation.signature = Some(signature_before_body(syntax_node, source)?);
        }
        let syntax = text(syntax_node, source)?;
        if syntax_node.child_by_field_name("type_parameters").is_some() {
            observation.attributes.push("generic_syntax".to_owned());
        }
        if syntax.trim_start().starts_with("async ")
            || syntax.trim_start().starts_with("pub async ")
        {
            observation.attributes.push("async".to_owned());
        }
        Ok(observation)
    }
}

impl SourceAdapter for RustAdapter {
    fn scope_kind(self, node: Node<'_>) -> &'static str {
        match node.kind() {
            "source_file" => "file",
            "closure_expression" => "closure",
            "function_item" => "function",
            "function_signature_item" => "function",
            "declaration_list" => match node.parent().map(|parent| parent.kind()) {
                Some("mod_item") => "module",
                Some("impl_item") => "impl",
                Some("trait_item") => "trait",
                _ => "declaration_list",
            },
            "block" => match node.parent().map(|parent| parent.kind()) {
                Some("function_item") => "function_body",
                Some("closure_expression") => "closure_body",
                _ => "block",
            },
            _ => "scope",
        }
    }

    fn scope_spelling(self, node: Node<'_>, source: &[u8]) -> Result<String, LanguageError> {
        match node.kind() {
            "source_file" => Ok("<file>".to_owned()),
            "declaration_list" => {
                if let Some(parent) = node.parent() {
                    if let Some(name) = parent.child_by_field_name("name") {
                        return Ok(text(name, source)?.to_owned());
                    }
                    if parent.kind() == "impl_item" {
                        return signature_before_body(parent, source);
                    }
                }
                Ok("<declarations>".to_owned())
            }
            "block" => {
                if let Some(parent) = node.parent()
                    && let Some(name) = parent.child_by_field_name("name")
                {
                    return Ok(text(name, source)?.to_owned());
                }
                Ok("<block>".to_owned())
            }
            "closure_expression" => Ok("<closure>".to_owned()),
            "function_item" | "function_signature_item" => node
                .child_by_field_name("name")
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| "<function>".to_owned())),
            _ => Ok(format!("<{}>", node.kind())),
        }
    }

    fn declarations(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        if capture_kind == "impl" {
            let Some(receiver) = node.child_by_field_name("type") else {
                return Ok(Vec::new());
            };
            let trait_name = node
                .child_by_field_name("trait")
                .map(|trait_node| text(trait_node, source).map(str::to_owned))
                .transpose()?;
            let receiver_text = text(receiver, source)?.to_owned();
            let spelling = trait_name.as_ref().map_or_else(
                || format!("impl {receiver_text}"),
                |trait_name| format!("impl {trait_name} for {receiver_text}"),
            );
            let range = node_range(receiver)?;
            let mut observation = new_observation(
                IdentityContext::new(language, relative_path),
                "declaration",
                "impl",
                spelling,
                range,
                node_range(node)?,
                node,
            );
            let own_scope_id = scopes
                .iter()
                .find(|scope| scope.syntax_range == observation.syntax_range)
                .map(|scope| scope.id.as_str());
            observation.scope_id = containing_scope_id(range, scopes, own_scope_id);
            observation.signature = Some(signature_before_body(node, source)?);
            observation.receiver_type = Some(receiver_text);
            if let Some(trait_name) = trait_name {
                observation.attributes.push(format!("trait:{trait_name}"));
            }
            return Ok(vec![observation]);
        }

        let mut effective_kind = capture_kind;
        if capture_kind == "function" {
            effective_kind = match nearest_ancestor(node, &["impl_item", "trait_item"])
                .map(|ancestor| ancestor.kind())
            {
                Some("impl_item") => "method",
                Some("trait_item") => "trait_method",
                _ => "function",
            };
        }
        let name_nodes = match capture_kind {
            "local" => node
                .child_by_field_name("pattern")
                .map(|pattern| named_descendants(pattern, &["identifier"]))
                .unwrap_or_default(),
            "self_parameter" => Vec::new(),
            "closure_parameter" => named_descendants(node, &["identifier"]),
            _ => node
                .child_by_field_name("name")
                .map_or_else(Vec::new, |name| vec![name]),
        };
        if capture_kind == "self_parameter" {
            let range = node_range(node)?;
            let mut observation = new_observation(
                IdentityContext::new(language, relative_path),
                "declaration",
                "parameter",
                "self".to_owned(),
                range,
                range,
                node,
            );
            set_context(&mut observation, scopes, self.container(node, source)?);
            return Ok(vec![observation]);
        }
        let kind = if capture_kind == "closure_parameter" {
            "parameter"
        } else {
            effective_kind
        };
        name_nodes
            .into_iter()
            .map(|name| {
                self.declaration_from_name(
                    IdentityContext::new(language, relative_path),
                    kind,
                    name,
                    node,
                    source,
                    scopes,
                )
            })
            .collect()
    }

    fn imports(
        self,
        language: LanguageId,
        relative_path: &Path,
        _capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let Some(argument) = node.child_by_field_name("argument") else {
            return Ok(Vec::new());
        };
        let mut leaves = Vec::new();
        expand_rust_use(argument, String::new(), source, &mut leaves)?;
        let syntax_range = node_range(node)?;
        let mut imports = Vec::new();
        for (path, alias, leaf) in leaves {
            let range = node_range(leaf)?;
            let mut import = new_observation(
                IdentityContext::new(language, relative_path),
                "import",
                "use",
                path,
                range,
                syntax_range,
                leaf,
            );
            import.alias = alias;
            import.scope_id = containing_scope_id(range, scopes, None);
            imports.push(import);
        }
        Ok(imports)
    }

    fn call(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Option<SyntaxObservation>, LanguageError> {
        let target = if capture_kind == "macro" {
            node.child_by_field_name("macro")
        } else {
            node.child_by_field_name("function")
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let range = node_range(target)?;
        let mut call = new_observation(
            IdentityContext::new(language, relative_path),
            "call",
            if capture_kind == "macro" {
                "macro"
            } else {
                "function"
            },
            text(target, source)?.to_owned(),
            range,
            node_range(node)?,
            node,
        );
        call.scope_id = containing_scope_id(range, scopes, None);
        call.container = self.container(node, source)?;
        call.resolution = ResolutionCategory::Unresolved;
        call.limitations.push(if capture_kind == "macro" {
            "macro_not_expanded".to_owned()
        } else {
            "call_target_not_resolved".to_owned()
        });
        Ok(Some(call))
    }

    fn exclude_reference(self, _capture_kind: &str, node: Node<'_>, _source: &[u8]) -> bool {
        nearest_ancestor(node, &["macro_invocation", "lifetime"]).is_some()
    }

    fn decorate_reference(
        self,
        _capture_kind: &str,
        _node: Node<'_>,
        _source: &[u8],
        _observation: &mut SyntaxObservation,
    ) -> Result<(), LanguageError> {
        Ok(())
    }

    fn diagnostics(self, conditions: &[SyntaxObservation]) -> Vec<Diagnostic> {
        let mut diagnostics = vec![Diagnostic {
            code: "rust_source_only_limits".to_owned(),
            message: "Rust extraction is syntax-only: macros are not expanded, build scripts are not run, and call targets are not resolved".to_owned(),
            severity: DiagnosticSeverity::Information,
            range: None,
        }];
        if !conditions.is_empty() {
            diagnostics.push(Diagnostic {
                code: "rust_cfg_unknown".to_owned(),
                message: "cfg/cfg_attr syntax was observed, but no active target or feature configuration was inferred".to_owned(),
                severity: DiagnosticSeverity::Warning,
                range: None,
            });
        }
        diagnostics
    }
}

fn combine_path(prefix: &str, suffix: &str) -> String {
    if prefix.is_empty() {
        suffix.to_owned()
    } else if suffix == "self" || suffix.is_empty() {
        prefix.to_owned()
    } else {
        format!("{prefix}::{suffix}")
    }
}

fn expand_rust_use<'tree>(
    node: Node<'tree>,
    prefix: String,
    source: &[u8],
    output: &mut Vec<(String, Option<String>, Node<'tree>)>,
) -> Result<(), LanguageError> {
    match node.kind() {
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                expand_rust_use(child, prefix.clone(), source, output)?;
            }
        }
        "scoped_use_list" => {
            let path = node
                .child_by_field_name("path")
                .map(|path| text(path, source).map(str::to_owned))
                .transpose()?
                .unwrap_or_default();
            let next_prefix = combine_path(&prefix, &path);
            if let Some(list) = node.child_by_field_name("list") {
                expand_rust_use(list, next_prefix, source, output)?;
            }
        }
        "use_as_clause" => {
            let Some(path_node) = node.child_by_field_name("path") else {
                return Ok(());
            };
            let alias = node
                .child_by_field_name("alias")
                .map(|alias| text(alias, source).map(str::to_owned))
                .transpose()?;
            let path = combine_path(&prefix, text(path_node, source)?);
            output.push((path, alias, node));
        }
        "self" => output.push((prefix, Some("self".to_owned()), node)),
        _ => {
            let path = combine_path(&prefix, text(node, source)?.trim());
            output.push((path, None, node));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct GoAdapter;

impl GoAdapter {
    fn receiver_type(
        self,
        method: Node<'_>,
        source: &[u8],
    ) -> Result<Option<String>, LanguageError> {
        let Some(receiver) = method.child_by_field_name("receiver") else {
            return Ok(None);
        };
        let parameter = receiver.named_child(0).unwrap_or(receiver);
        parameter
            .child_by_field_name("type")
            .map(|receiver_type| text(receiver_type, source).map(str::to_owned))
            .transpose()
    }

    fn container(self, node: Node<'_>, source: &[u8]) -> Result<Option<String>, LanguageError> {
        let method = if node.kind() == "method_declaration" {
            Some(node)
        } else {
            nearest_ancestor(node, &["method_declaration"])
        };
        if let Some(method) = method
            && let Some(receiver_type) = self.receiver_type(method, source)?
        {
            return Ok(Some(receiver_type));
        }
        if let Some(type_spec) = nearest_ancestor(node, &["type_spec", "type_alias"])
            && let Some(name) = type_spec.child_by_field_name("name")
        {
            return Ok(Some(text(name, source)?.to_owned()));
        }
        Ok(None)
    }

    fn declaration_from_name(
        self,
        identity: IdentityContext<'_>,
        kind: &str,
        name_node: Node<'_>,
        syntax_node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<SyntaxObservation, LanguageError> {
        let range = node_range(name_node)?;
        let mut observation = new_observation(
            identity,
            "declaration",
            kind,
            text(name_node, source)?.to_owned(),
            range,
            node_range(syntax_node)?,
            name_node,
        );
        set_context(
            &mut observation,
            scopes,
            self.container(syntax_node, source)?,
        );
        if matches!(kind, "function" | "method" | "interface_method") {
            observation.signature = Some(signature_before_body(syntax_node, source)?);
        }
        if kind == "method"
            && let Some(receiver_type) = self.receiver_type(syntax_node, source)?
        {
            observation.receiver_type = Some(receiver_type);
        }
        if syntax_node.child_by_field_name("type_parameters").is_some() {
            observation.attributes.push("generic_syntax".to_owned());
        }
        Ok(observation)
    }
}

impl SourceAdapter for GoAdapter {
    fn scope_kind(self, node: Node<'_>) -> &'static str {
        match node.kind() {
            "source_file" => "file",
            "func_literal" => "closure",
            "function_declaration" => "function",
            "method_declaration" => "method",
            "method_elem" => "interface_method",
            "block" => match node.parent().map(|parent| parent.kind()) {
                Some("function_declaration") => "function_body",
                Some("method_declaration") => "method_body",
                Some("func_literal") => "closure_body",
                _ => "block",
            },
            _ => "scope",
        }
    }

    fn scope_spelling(self, node: Node<'_>, source: &[u8]) -> Result<String, LanguageError> {
        match node.kind() {
            "source_file" => Ok("<file>".to_owned()),
            "func_literal" => Ok("<closure>".to_owned()),
            "function_declaration" | "method_declaration" | "method_elem" => node
                .child_by_field_name("name")
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| "<function>".to_owned())),
            "block" => {
                if let Some(parent) = node.parent()
                    && let Some(name) = parent.child_by_field_name("name")
                {
                    return Ok(text(name, source)?.to_owned());
                }
                Ok("<block>".to_owned())
            }
            _ => Ok(format!("<{}>", node.kind())),
        }
    }

    fn declarations(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let name_nodes = match capture_kind {
            "local" => node
                .child_by_field_name("left")
                .map(|left| named_descendants(left, &["identifier"]))
                .unwrap_or_default(),
            "field" => field_nodes(node, "name"),
            "const" | "variable" | "parameter" => field_nodes(node, "name"),
            _ => node
                .child_by_field_name("name")
                .map_or_else(Vec::new, |name| vec![name]),
        };
        if capture_kind == "field" && name_nodes.is_empty() {
            let Some(type_node) = node.child_by_field_name("type") else {
                return Ok(Vec::new());
            };
            let mut embedded = self.declaration_from_name(
                IdentityContext::new(language, relative_path),
                "embedded_field",
                type_node,
                node,
                source,
                scopes,
            )?;
            embedded.attributes.push("embedded".to_owned());
            return Ok(vec![embedded]);
        }
        name_nodes
            .into_iter()
            .map(|name| {
                self.declaration_from_name(
                    IdentityContext::new(language, relative_path),
                    capture_kind,
                    name,
                    node,
                    source,
                    scopes,
                )
            })
            .collect()
    }

    fn imports(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        if capture_kind == "package" {
            let Some(name) = node.named_child(0) else {
                return Ok(Vec::new());
            };
            let range = node_range(name)?;
            let mut package = new_observation(
                IdentityContext::new(language, relative_path),
                "import",
                "package",
                text(name, source)?.to_owned(),
                range,
                node_range(node)?,
                name,
            );
            package.scope_id = containing_scope_id(range, scopes, None);
            return Ok(vec![package]);
        }
        let Some(path_node) = node.child_by_field_name("path") else {
            return Ok(Vec::new());
        };
        let raw_path = text(path_node, source)?;
        let path = raw_path
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| {
                raw_path
                    .strip_prefix('`')
                    .and_then(|value| value.strip_suffix('`'))
            })
            .unwrap_or(raw_path)
            .to_owned();
        let explicit_alias = node
            .child_by_field_name("name")
            .map(|alias| text(alias, source).map(str::to_owned))
            .transpose()?;
        let (alias, alias_is_derived) = match explicit_alias {
            Some(alias) => (Some(alias), false),
            None => (path.rsplit('/').next().map(str::to_owned), true),
        };
        let range = node_range(path_node)?;
        let mut import = new_observation(
            IdentityContext::new(language, relative_path),
            "import",
            "import",
            path,
            range,
            node_range(node)?,
            node,
        );
        import.alias = alias;
        if alias_is_derived {
            import
                .limitations
                .push("default_alias_derived_from_import_path".to_owned());
        }
        import.scope_id = containing_scope_id(range, scopes, None);
        Ok(vec![import])
    }

    fn call(
        self,
        language: LanguageId,
        relative_path: &Path,
        _capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Option<SyntaxObservation>, LanguageError> {
        let Some(function) = node.child_by_field_name("function") else {
            return Ok(None);
        };
        let range = node_range(function)?;
        let spelling = text(function, source)?.to_owned();
        let mut call = new_observation(
            IdentityContext::new(language, relative_path),
            "call",
            "function",
            spelling.clone(),
            range,
            node_range(node)?,
            node,
        );
        call.scope_id = containing_scope_id(range, scopes, None);
        call.container = self.container(node, source)?;
        call.resolution = ResolutionCategory::Unresolved;
        call.limitations.push("call_target_not_resolved".to_owned());
        if spelling
            .rsplit(['.', '/'])
            .next()
            .and_then(|name| name.chars().next())
            .is_some_and(char::is_uppercase)
        {
            call.attributes.push("constructor_like_syntax".to_owned());
        }
        Ok(Some(call))
    }

    fn exclude_reference(self, _capture_kind: &str, node: Node<'_>, source: &[u8]) -> bool {
        text(node, source).is_ok_and(|spelling| spelling == "_")
    }

    fn decorate_reference(
        self,
        _capture_kind: &str,
        _node: Node<'_>,
        _source: &[u8],
        _observation: &mut SyntaxObservation,
    ) -> Result<(), LanguageError> {
        Ok(())
    }

    fn diagnostics(self, conditions: &[SyntaxObservation]) -> Vec<Diagnostic> {
        let mut diagnostics = vec![Diagnostic {
            code: "go_source_only_limits".to_owned(),
            message: "Go extraction is syntax-only: packages are not loaded, build tools are not run, interface satisfaction is not inferred, and call targets are not resolved".to_owned(),
            severity: DiagnosticSeverity::Information,
            range: None,
        }];
        if !conditions.is_empty() {
            diagnostics.push(Diagnostic {
                code: "go_build_constraints_unknown".to_owned(),
                message: "Go build-tag syntax was observed, but no active GOOS, GOARCH, or tag set was inferred".to_owned(),
                severity: DiagnosticSeverity::Warning,
                range: None,
            });
        }
        diagnostics
    }
}

#[derive(Clone, Copy)]
struct CSharpAdapter;

impl CSharpAdapter {
    fn namespace_name(
        self,
        mut node: Node<'_>,
        source: &[u8],
    ) -> Result<Option<String>, LanguageError> {
        while let Some(parent) = node.parent() {
            if matches!(
                parent.kind(),
                "namespace_declaration" | "file_scoped_namespace_declaration"
            ) && let Some(name) = parent.child_by_field_name("name")
            {
                return Ok(Some(text(name, source)?.to_owned()));
            }
            node = parent;
        }
        let mut root = node;
        while let Some(parent) = root.parent() {
            root = parent;
        }
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() == "file_scoped_namespace_declaration"
                && let Some(name) = child.child_by_field_name("name")
            {
                return Ok(Some(text(name, source)?.to_owned()));
            }
        }
        Ok(None)
    }

    fn container(self, node: Node<'_>, source: &[u8]) -> Result<Option<String>, LanguageError> {
        let declaration = nearest_ancestor(
            node,
            &[
                "class_declaration",
                "struct_declaration",
                "interface_declaration",
                "enum_declaration",
                "record_declaration",
                "method_declaration",
                "constructor_declaration",
                "local_function_statement",
                "property_declaration",
            ],
        );
        if let Some(declaration) = declaration
            && let Some(name) = declaration.child_by_field_name("name")
        {
            return Ok(Some(text(name, source)?.to_owned()));
        }
        self.namespace_name(node, source)
    }

    fn attached_attributes(
        self,
        node: Node<'_>,
        source: &[u8],
    ) -> Result<Vec<String>, LanguageError> {
        let mut output = Vec::new();
        for list in immediate_named_children(node, "attribute_list") {
            for attribute in named_descendants(list, &["attribute"]) {
                if let Some(name) = attribute.child_by_field_name("name") {
                    output.push(text(name, source)?.to_owned());
                }
            }
        }
        Ok(output)
    }

    fn declaration_from_name(
        self,
        identity: IdentityContext<'_>,
        kind: &str,
        name_node: Node<'_>,
        syntax_node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<SyntaxObservation, LanguageError> {
        let range = node_range(name_node)?;
        let spelling = text(name_node, source)?.to_owned();
        let mut observation = new_observation(
            identity,
            "declaration",
            kind,
            spelling.clone(),
            range,
            node_range(syntax_node)?,
            name_node,
        );
        set_context(
            &mut observation,
            scopes,
            self.container(syntax_node, source)?,
        );
        if matches!(
            kind,
            "method" | "constructor" | "local_function" | "accessor"
        ) {
            observation.signature = Some(signature_before_body(syntax_node, source)?);
        }
        if syntax_node.child_by_field_name("type_parameters").is_some()
            || !immediate_named_children(syntax_node, "type_parameter_list").is_empty()
        {
            observation.attributes.push("generic_syntax".to_owned());
        }
        let modifiers = immediate_named_children(syntax_node, "modifier")
            .into_iter()
            .map(|modifier| text(modifier, source).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?;
        if modifiers.iter().any(|modifier| modifier == "async") {
            observation.attributes.push("async".to_owned());
        }
        for attribute in self.attached_attributes(syntax_node, source)? {
            observation
                .attributes
                .push(format!("attribute:{attribute}"));
        }
        if matches!(kind, "class" | "struct" | "interface" | "record")
            && modifiers.iter().any(|modifier| modifier == "partial")
        {
            observation
                .attributes
                .push("partial_declaration".to_owned());
            let namespace = self
                .namespace_name(syntax_node, source)?
                .unwrap_or_else(|| "<global>".to_owned());
            let outer = self.container(syntax_node, source)?;
            let group = match outer.as_deref() {
                Some(outer) if outer != namespace => format!("{namespace}.{outer}.{spelling}"),
                _ => format!("{namespace}.{spelling}"),
            };
            observation
                .attributes
                .push(format!("partial_group_hint:{group}"));
            observation
                .limitations
                .push("partial_group_not_semantically_confirmed".to_owned());
        }
        if kind == "method" {
            observation
                .attributes
                .push("overload_identity_preserved".to_owned());
            if nearest_ancestor(syntax_node, &["interface_declaration"]).is_some() {
                observation
                    .attributes
                    .push("interface_member_syntax".to_owned());
                observation
                    .limitations
                    .push("implementation_not_inferred".to_owned());
            }
            if node_has_extension_receiver(syntax_node, source)? {
                observation
                    .attributes
                    .push("extension_method_syntax".to_owned());
                observation
                    .limitations
                    .push("extension_dispatch_not_resolved".to_owned());
            }
        }
        Ok(observation)
    }
}

impl SourceAdapter for CSharpAdapter {
    fn scope_kind(self, node: Node<'_>) -> &'static str {
        match node.kind() {
            "compilation_unit" => "file",
            "namespace_declaration" | "file_scoped_namespace_declaration" => "namespace",
            "declaration_list" => "declaration_list",
            "method_declaration" => "method",
            "constructor_declaration" => "constructor",
            "local_function_statement" => "local_function",
            "accessor_declaration" => "accessor",
            "block" => "block",
            _ => "scope",
        }
    }

    fn scope_spelling(self, node: Node<'_>, source: &[u8]) -> Result<String, LanguageError> {
        match node.kind() {
            "compilation_unit" => Ok("<file>".to_owned()),
            "namespace_declaration"
            | "file_scoped_namespace_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "local_function_statement"
            | "accessor_declaration" => node
                .child_by_field_name("name")
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| format!("<{}>", node.kind()))),
            "declaration_list" => node
                .parent()
                .and_then(|parent| parent.child_by_field_name("name"))
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| "<declarations>".to_owned())),
            "block" => Ok("<block>".to_owned()),
            _ => Ok(format!("<{}>", node.kind())),
        }
    }

    fn declarations(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let identity = IdentityContext::new(language, relative_path);
        let kind = if capture_kind == "file_namespace" {
            "namespace"
        } else {
            capture_kind
        };
        let names = match capture_kind {
            "field" | "local" => named_descendants(node, &["variable_declarator"])
                .into_iter()
                .filter_map(|declarator| declarator.child_by_field_name("name"))
                .collect(),
            "accessor" => node
                .child_by_field_name("name")
                .map_or_else(Vec::new, |name| vec![name]),
            _ => node
                .child_by_field_name("name")
                .map_or_else(Vec::new, |name| vec![name]),
        };
        names
            .into_iter()
            .map(|name| self.declaration_from_name(identity, kind, name, node, source, scopes))
            .collect()
    }

    fn imports(
        self,
        language: LanguageId,
        relative_path: &Path,
        _capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let named_children = {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect::<Vec<_>>()
        };
        let Some(target) = named_children.last().copied() else {
            return Ok(Vec::new());
        };
        let range = node_range(target)?;
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "import",
            "using",
            text(target, source)?.to_owned(),
            range,
            node_range(node)?,
            target,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        observation.alias = node
            .child_by_field_name("name")
            .map(|alias| text(alias, source).map(str::to_owned))
            .transpose()?;
        let syntax = text(node, source)?;
        if syntax.split_whitespace().any(|word| word == "static") {
            observation.attributes.push("static_import".to_owned());
        }
        if syntax.trim_start().starts_with("global ") {
            observation.attributes.push("global_using".to_owned());
        }
        observation
            .limitations
            .push("namespace_and_type_resolution_not_performed".to_owned());
        Ok(vec![observation])
    }

    fn call(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Option<SyntaxObservation>, LanguageError> {
        let (kind, identity_node, spelling, target_expression) = match capture_kind {
            "constructor" => {
                let Some(target) = node.child_by_field_name("type") else {
                    return Ok(None);
                };
                (
                    "constructor",
                    target,
                    text(target, source)?.to_owned(),
                    target,
                )
            }
            "implicit_constructor" => ("constructor", node, text(node, source)?.to_owned(), node),
            _ => {
                let Some(target) = node.child_by_field_name("function") else {
                    return Ok(None);
                };
                let identity_node = target.child_by_field_name("name").unwrap_or(target);
                let name = text(identity_node, source)?.to_owned();
                let kind = if target.kind() == "member_access_expression" {
                    "method"
                } else {
                    "function"
                };
                (kind, identity_node, name, target)
            }
        };
        let range = node_range(identity_node)?;
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "call",
            kind,
            spelling,
            range,
            node_range(node)?,
            node,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        observation.container = self.container(node, source)?;
        observation.resolution = ResolutionCategory::Unresolved;
        if target_expression.kind() == "member_access_expression"
            && let Some(receiver) = target_expression.child_by_field_name("expression")
        {
            observation.receiver_type = Some(text(receiver, source)?.to_owned());
            observation
                .attributes
                .push("receiver_expression_source".to_owned());
            observation
                .limitations
                .push("receiver_type_not_inferred".to_owned());
            observation
                .limitations
                .push("interface_virtual_or_extension_dispatch_not_resolved".to_owned());
        } else if kind == "constructor" {
            observation
                .limitations
                .push("constructor_target_not_resolved".to_owned());
        } else {
            observation
                .limitations
                .push("call_target_not_resolved".to_owned());
        }
        if nearest_ancestor(node, &["await_expression"]).is_some() {
            observation.attributes.push("awaited".to_owned());
        }
        Ok(Some(observation))
    }

    fn exclude_reference(self, capture_kind: &str, node: Node<'_>, _source: &[u8]) -> bool {
        capture_kind == "identifier" && nearest_ancestor(node, &["attribute"]).is_some()
    }

    fn decorate_reference(
        self,
        capture_kind: &str,
        _node: Node<'_>,
        _source: &[u8],
        observation: &mut SyntaxObservation,
    ) -> Result<(), LanguageError> {
        if capture_kind == "attribute" {
            observation.attributes.push("role:attribute".to_owned());
            observation
                .limitations
                .push("attribute_behavior_and_arguments_not_evaluated".to_owned());
        }
        Ok(())
    }

    fn diagnostics(self, conditions: &[SyntaxObservation]) -> Vec<Diagnostic> {
        let mut diagnostics = vec![Diagnostic {
            code: "csharp_source_only_limits".to_owned(),
            message: "C# extraction is syntax-only: no .NET, MSBuild, project evaluation, type checking, overload selection, DI inference, or dispatch resolution is performed; recovered ERROR subtrees are not treated as fully understood".to_owned(),
            severity: DiagnosticSeverity::Information,
            range: None,
        }];
        if !conditions.is_empty() {
            diagnostics.push(Diagnostic {
                code: "csharp_preprocessor_configuration_unknown".to_owned(),
                message: "C# preprocessor conditions were observed, but no active symbol configuration was inferred".to_owned(),
                severity: DiagnosticSeverity::Warning,
                range: None,
            });
        }
        diagnostics
    }
}

#[derive(Clone, Copy)]
struct JavaAdapter;

impl JavaAdapter {
    fn container(self, node: Node<'_>, source: &[u8]) -> Result<Option<String>, LanguageError> {
        let Some(container) = nearest_ancestor(
            node,
            &[
                "class_declaration",
                "interface_declaration",
                "enum_declaration",
                "record_declaration",
                "annotation_type_declaration",
                "method_declaration",
                "constructor_declaration",
                "compact_constructor_declaration",
            ],
        ) else {
            return Ok(None);
        };
        container
            .child_by_field_name("name")
            .map(|name| text(name, source).map(str::to_owned))
            .transpose()
    }

    fn attached_annotations(
        self,
        node: Node<'_>,
        source: &[u8],
    ) -> Result<Vec<String>, LanguageError> {
        let mut annotation_nodes = Vec::new();
        for modifiers in immediate_named_children(node, "modifiers") {
            annotation_nodes.extend(named_descendants(
                modifiers,
                &["annotation", "marker_annotation"],
            ));
        }
        annotation_nodes.extend(immediate_named_children(node, "annotation"));
        annotation_nodes.extend(immediate_named_children(node, "marker_annotation"));
        annotation_nodes
            .into_iter()
            .filter_map(|annotation| annotation.child_by_field_name("name"))
            .map(|name| text(name, source).map(str::to_owned))
            .collect()
    }

    fn declaration_from_name(
        self,
        identity: IdentityContext<'_>,
        kind: &str,
        name_node: Node<'_>,
        syntax_node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<SyntaxObservation, LanguageError> {
        let range = node_range(name_node)?;
        let mut observation = new_observation(
            identity,
            "declaration",
            kind,
            text(name_node, source)?.to_owned(),
            range,
            node_range(syntax_node)?,
            name_node,
        );
        set_context(
            &mut observation,
            scopes,
            self.container(syntax_node, source)?,
        );
        if matches!(kind, "method" | "constructor" | "annotation_element") {
            observation.signature = Some(signature_before_body(syntax_node, source)?);
        }
        if syntax_node.child_by_field_name("type_parameters").is_some() {
            observation.attributes.push("generic_syntax".to_owned());
        }
        for annotation in self.attached_annotations(syntax_node, source)? {
            observation
                .attributes
                .push(format!("annotation:{annotation}"));
        }
        if kind == "method" {
            observation
                .attributes
                .push("overload_identity_preserved".to_owned());
            if nearest_ancestor(syntax_node, &["interface_declaration"]).is_some() {
                observation
                    .attributes
                    .push("interface_member_syntax".to_owned());
                observation
                    .limitations
                    .push("implementation_not_inferred".to_owned());
            }
        }
        if matches!(
            kind,
            "class" | "interface" | "enum" | "record" | "annotation_type"
        ) && nearest_ancestor(
            syntax_node,
            &[
                "class_declaration",
                "interface_declaration",
                "enum_declaration",
                "record_declaration",
                "annotation_type_declaration",
            ],
        )
        .is_some()
        {
            observation.attributes.push("nested_type".to_owned());
        }
        Ok(observation)
    }
}

impl SourceAdapter for JavaAdapter {
    fn scope_kind(self, node: Node<'_>) -> &'static str {
        match node.kind() {
            "program" => "file",
            "class_body" => "class",
            "interface_body" => "interface",
            "enum_body" => "enum",
            "annotation_type_body" => "annotation_type",
            "method_declaration" => "method",
            "constructor_declaration" | "compact_constructor_declaration" => "constructor",
            "constructor_body" => "constructor_body",
            "lambda_expression" => "lambda",
            "block" => "block",
            _ => "scope",
        }
    }

    fn scope_spelling(self, node: Node<'_>, source: &[u8]) -> Result<String, LanguageError> {
        match node.kind() {
            "program" => Ok("<file>".to_owned()),
            "class_body" | "interface_body" | "enum_body" | "annotation_type_body" => node
                .parent()
                .and_then(|parent| parent.child_by_field_name("name"))
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| format!("<{}>", node.kind()))),
            "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration" => node
                .child_by_field_name("name")
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| "<callable>".to_owned())),
            "lambda_expression" => Ok("<lambda>".to_owned()),
            "constructor_body" => Ok("<constructor-body>".to_owned()),
            "block" => Ok("<block>".to_owned()),
            _ => Ok(format!("<{}>", node.kind())),
        }
    }

    fn declarations(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let identity = IdentityContext::new(language, relative_path);
        let names = match capture_kind {
            "field" | "local" => named_descendants(node, &["variable_declarator"])
                .into_iter()
                .filter_map(|declarator| declarator.child_by_field_name("name"))
                .collect(),
            "type_parameter" => first_named_descendant(node, &["type_identifier"])
                .map_or_else(Vec::new, |name| vec![name]),
            "lambda_parameter" => node
                .child_by_field_name("parameters")
                .map(|parameters| {
                    if matches!(parameters.kind(), "identifier" | "underscore_pattern") {
                        vec![parameters]
                    } else {
                        named_descendants(parameters, &["identifier", "underscore_pattern"])
                    }
                })
                .unwrap_or_default(),
            _ => node
                .child_by_field_name("name")
                .map_or_else(Vec::new, |name| vec![name]),
        };
        let kind = if capture_kind == "lambda_parameter" {
            "parameter"
        } else {
            capture_kind
        };
        names
            .into_iter()
            .map(|name| self.declaration_from_name(identity, kind, name, node, source, scopes))
            .collect()
    }

    fn imports(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let syntax = text(node, source)?.trim();
        let target = {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find(|child| matches!(child.kind(), "identifier" | "scoped_identifier"))
        };
        let Some(target) = target else {
            return Ok(Vec::new());
        };
        let spelling = text(target, source)?.to_owned();
        let range = node_range(target)?;
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "import",
            capture_kind,
            spelling,
            range,
            node_range(node)?,
            target,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        if capture_kind == "import" && syntax.starts_with("import static ") {
            observation.attributes.push("static_import".to_owned());
        }
        if capture_kind == "import" && !immediate_named_children(node, "asterisk").is_empty() {
            observation.attributes.push("wildcard_import".to_owned());
        }
        if capture_kind == "import" {
            observation
                .limitations
                .push("classpath_and_import_resolution_not_performed".to_owned());
        }
        Ok(vec![observation])
    }

    fn call(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Option<SyntaxObservation>, LanguageError> {
        let target = if capture_kind == "constructor" {
            node.child_by_field_name("type")
        } else {
            node.child_by_field_name("name")
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let range = node_range(target)?;
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "call",
            capture_kind,
            text(target, source)?.to_owned(),
            range,
            node_range(node)?,
            node,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        observation.container = self.container(node, source)?;
        observation.resolution = ResolutionCategory::Unresolved;
        if capture_kind == "method"
            && let Some(receiver) = node.child_by_field_name("object")
        {
            observation.receiver_type = Some(text(receiver, source)?.to_owned());
            observation
                .attributes
                .push("receiver_expression_source".to_owned());
            observation
                .limitations
                .push("receiver_type_not_inferred".to_owned());
            observation
                .limitations
                .push("interface_or_virtual_dispatch_not_resolved".to_owned());
        } else if capture_kind == "constructor" {
            observation
                .limitations
                .push("constructor_target_not_resolved".to_owned());
        } else {
            observation
                .limitations
                .push("call_target_not_resolved".to_owned());
        }
        Ok(Some(observation))
    }

    fn exclude_reference(self, capture_kind: &str, node: Node<'_>, _source: &[u8]) -> bool {
        capture_kind == "identifier"
            && nearest_ancestor(
                node,
                &["annotation", "marker_annotation", "method_reference"],
            )
            .is_some()
    }

    fn decorate_reference(
        self,
        capture_kind: &str,
        _node: Node<'_>,
        _source: &[u8],
        observation: &mut SyntaxObservation,
    ) -> Result<(), LanguageError> {
        match capture_kind {
            "annotation" => {
                observation.attributes.push("role:annotation".to_owned());
                observation
                    .limitations
                    .push("annotation_behavior_and_arguments_not_evaluated".to_owned());
            }
            "method_reference" => {
                observation
                    .attributes
                    .push("method_reference_syntax".to_owned());
                observation
                    .limitations
                    .push("method_reference_not_a_call_edge".to_owned());
                observation
                    .limitations
                    .push("method_reference_target_not_resolved".to_owned());
            }
            _ => {}
        }
        Ok(())
    }

    fn diagnostics(self, _conditions: &[SyntaxObservation]) -> Vec<Diagnostic> {
        vec![Diagnostic {
            code: "java_source_only_limits".to_owned(),
            message: "Java extraction is syntax-only: no JVM, compiler, Gradle/Maven, classpath loading, annotation processing, overload selection, DI inference, or dispatch resolution is performed; recovered ERROR subtrees are not treated as fully understood".to_owned(),
            severity: DiagnosticSeverity::Information,
            range: None,
        }]
    }
}

fn node_has_extension_receiver(node: Node<'_>, source: &[u8]) -> Result<bool, LanguageError> {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return Ok(false);
    };
    let Some(first_parameter) = parameters.named_child(0) else {
        return Ok(false);
    };
    for modifier in immediate_named_children(first_parameter, "modifier") {
        if text(modifier, source)? == "this" {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Clone, Copy)]
struct EcmaAdapter {
    typescript: bool,
    jsx: bool,
}

impl EcmaAdapter {
    const fn new(language: LanguageId) -> Self {
        Self {
            typescript: matches!(language, LanguageId::TypeScript | LanguageId::Tsx),
            jsx: matches!(language, LanguageId::Jsx | LanguageId::Tsx),
        }
    }

    fn container(self, node: Node<'_>, source: &[u8]) -> Result<Option<String>, LanguageError> {
        let Some(container) = nearest_ancestor(
            node,
            &[
                "class_declaration",
                "abstract_class_declaration",
                "interface_declaration",
                "internal_module",
                "function_declaration",
                "generator_function_declaration",
            ],
        ) else {
            return Ok(None);
        };
        container
            .child_by_field_name("name")
            .map(|name| text(name, source).map(str::to_owned))
            .transpose()
    }

    fn declaration_from_name(
        self,
        identity: IdentityContext<'_>,
        kind: &str,
        name_node: Node<'_>,
        syntax_node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<SyntaxObservation, LanguageError> {
        let range = node_range(name_node)?;
        let mut observation = new_observation(
            identity,
            "declaration",
            kind,
            text(name_node, source)?.to_owned(),
            range,
            node_range(syntax_node)?,
            name_node,
        );
        set_context(
            &mut observation,
            scopes,
            self.container(syntax_node, source)?,
        );
        if matches!(
            kind,
            "function"
                | "function_binding"
                | "function_overload"
                | "method"
                | "method_signature"
                | "constructor"
        ) {
            let signature_node = if kind == "function_binding" {
                syntax_node
                    .child_by_field_name("value")
                    .unwrap_or(syntax_node)
            } else {
                syntax_node
            };
            observation.signature = Some(signature_before_body(signature_node, source)?);
        }
        let syntax = text(syntax_node, source)?;
        let generic_node = if kind == "function_binding" {
            syntax_node
                .child_by_field_name("value")
                .unwrap_or(syntax_node)
        } else {
            syntax_node
        };
        if generic_node
            .child_by_field_name("type_parameters")
            .is_some()
        {
            observation.attributes.push("generic_syntax".to_owned());
        }
        let signature_prefix = observation
            .signature
            .as_deref()
            .unwrap_or(syntax)
            .split(['(', '='])
            .next()
            .unwrap_or_default();
        if matches!(
            kind,
            "function" | "function_binding" | "method" | "constructor"
        ) && signature_prefix
            .split_whitespace()
            .any(|word| word == "async")
        {
            observation.attributes.push("async".to_owned());
        }
        for decorator in attached_decorators(syntax_node) {
            observation
                .attributes
                .push(format!("decorator:{}", text(decorator, source)?.trim()));
        }
        let mut export_parent = syntax_node.parent();
        while export_parent.is_some_and(|parent| {
            matches!(
                parent.kind(),
                "lexical_declaration" | "variable_declaration"
            )
        }) {
            export_parent = export_parent.and_then(|parent| parent.parent());
        }
        if export_parent.is_some_and(|parent| parent.kind() == "export_statement") {
            observation.attributes.push("exported".to_owned());
        }
        match kind {
            "interface" | "type_alias" | "method_signature" => {
                observation.attributes.push("role:type".to_owned());
            }
            "class" | "enum" => {
                observation.attributes.push("role:type".to_owned());
                observation.attributes.push("role:value".to_owned());
            }
            "namespace" => {
                observation.attributes.push("role:namespace".to_owned());
                observation.attributes.push("role:value".to_owned());
            }
            _ => observation.attributes.push("role:value".to_owned()),
        }
        if kind == "function_overload" {
            observation
                .attributes
                .push("overload_declaration".to_owned());
            observation
                .limitations
                .push("overload_implementation_not_linked".to_owned());
        }
        Ok(observation)
    }

    fn esm_imports(
        self,
        identity: IdentityContext<'_>,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let Some((module, source_node)) = module_source(node, source)? else {
            return Ok(Vec::new());
        };
        let statement = text(node, source)?;
        let statement_type_only = statement.trim_start().starts_with("import type ");
        let mut bindings = Vec::new();
        if let Some(clause) = node
            .named_children(&mut node.walk())
            .find(|child| matches!(child.kind(), "import_clause" | "import_require_clause"))
        {
            let mut cursor = clause.walk();
            for child in clause.named_children(&mut cursor) {
                match child.kind() {
                    "identifier" => bindings.push((
                        child,
                        "default".to_owned(),
                        text(child, source)?.to_owned(),
                        statement_type_only,
                    )),
                    "namespace_import" => {
                        if let Some(local) = first_named_descendant(child, &["identifier"]) {
                            bindings.push((
                                child,
                                "*".to_owned(),
                                text(local, source)?.to_owned(),
                                statement_type_only,
                            ));
                        }
                    }
                    "named_imports" => {
                        for specifier in named_descendants(child, &["import_specifier"]) {
                            let Some(imported) = specifier.child_by_field_name("name") else {
                                continue;
                            };
                            let local = specifier.child_by_field_name("alias").unwrap_or(imported);
                            let specifier_type_only =
                                text(specifier, source)?.trim_start().starts_with("type ");
                            bindings.push((
                                specifier,
                                text(imported, source)?.to_owned(),
                                text(local, source)?.to_owned(),
                                statement_type_only || specifier_type_only,
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        if bindings.is_empty() {
            bindings.push((
                source_node,
                "<side-effect>".to_owned(),
                String::new(),
                false,
            ));
        }
        bindings
            .into_iter()
            .map(|(binding_node, imported, local, type_only)| {
                let range = node_range(binding_node)?;
                let mut observation = new_observation(
                    identity,
                    "import",
                    "esm_import",
                    module.clone(),
                    range,
                    node_range(node)?,
                    binding_node,
                );
                observation.scope_id = containing_scope_id(range, scopes, None);
                observation.alias = (!local.is_empty()).then_some(local);
                observation.attributes.push(format!("imported:{imported}"));
                observation.attributes.push(if type_only {
                    "role:type".to_owned()
                } else {
                    "role:value".to_owned()
                });
                observation
                    .limitations
                    .push("module_resolution_not_performed".to_owned());
                Ok(observation)
            })
            .collect()
    }

    fn esm_exports(
        self,
        identity: IdentityContext<'_>,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let module = module_source(node, source)?;
        let statement = text(node, source)?;
        let type_only = statement.trim_start().starts_with("export type ");
        let specifiers = named_descendants(node, &["export_specifier"]);
        let mut exports = Vec::new();
        for specifier in specifiers {
            let Some(local) = specifier.child_by_field_name("name") else {
                continue;
            };
            let exported = specifier.child_by_field_name("alias").unwrap_or(local);
            let range = node_range(local)?;
            let mut observation = new_observation(
                identity,
                "import",
                if module.is_some() {
                    "reexport"
                } else {
                    "export"
                },
                module.as_ref().map_or_else(
                    || text(local, source).map(str::to_owned),
                    |value| Ok(value.0.clone()),
                )?,
                range,
                node_range(node)?,
                specifier,
            );
            observation.scope_id = containing_scope_id(range, scopes, None);
            observation.alias = Some(text(exported, source)?.to_owned());
            observation
                .attributes
                .push(format!("local:{}", text(local, source)?));
            observation.attributes.push(if type_only {
                "role:type".to_owned()
            } else {
                "role:value".to_owned()
            });
            if module.is_some() {
                observation
                    .limitations
                    .push("reexport_cycle_guard_required".to_owned());
                observation
                    .limitations
                    .push("module_resolution_not_performed".to_owned());
            }
            exports.push(observation);
        }
        if exports.is_empty() {
            let (spelling, identity_node) = if let Some((module, source_node)) = module {
                (module, source_node)
            } else if let Some(declaration) = node.child_by_field_name("declaration") {
                let name = declaration.child_by_field_name("name").or_else(|| {
                    first_named_descendant(declaration, &["identifier", "type_identifier"])
                });
                match name {
                    Some(name) => (text(name, source)?.to_owned(), name),
                    None => ("default".to_owned(), declaration),
                }
            } else if let Some(value) = node.child_by_field_name("value") {
                (text(value, source)?.to_owned(), value)
            } else if statement.trim_start().starts_with("export =") {
                match first_named_descendant(node, &["identifier", "type_identifier"]) {
                    Some(value) => (text(value, source)?.to_owned(), value),
                    None => ("<missing>".to_owned(), node),
                }
            } else {
                ("default".to_owned(), node)
            };
            let range = node_range(identity_node)?;
            let mut observation = new_observation(
                identity,
                "import",
                if node.child_by_field_name("source").is_some() {
                    "reexport"
                } else {
                    "export"
                },
                spelling,
                range,
                node_range(node)?,
                identity_node,
            );
            observation.scope_id = containing_scope_id(range, scopes, None);
            observation.alias = statement.contains("default").then(|| "default".to_owned());
            observation.attributes.push(if type_only {
                "role:type".to_owned()
            } else {
                "role:value".to_owned()
            });
            if node.child_by_field_name("source").is_some() {
                observation
                    .limitations
                    .push("reexport_cycle_guard_required".to_owned());
                observation
                    .limitations
                    .push("module_resolution_not_performed".to_owned());
            }
            exports.push(observation);
        }
        Ok(exports)
    }

    fn module_call_import(
        self,
        identity: IdentityContext<'_>,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let Some(function) = node.child_by_field_name("function") else {
            return Ok(Vec::new());
        };
        let function_text = text(function, source)?;
        let kind = match (function.kind(), function_text) {
            ("import", _) => "dynamic_import",
            (_, "require") => "commonjs_require",
            _ => return Ok(Vec::new()),
        };
        let argument = node
            .child_by_field_name("arguments")
            .and_then(|arguments| arguments.named_child(0));
        let (spelling, range, identity_node, is_static) = if let Some(argument) = argument {
            (
                if argument.kind() == "string" {
                    unquote(text(argument, source)?)
                } else {
                    text(argument, source)?.to_owned()
                },
                node_range(argument)?,
                argument,
                argument.kind() == "string",
            )
        } else {
            ("<missing>".to_owned(), node_range(node)?, node, false)
        };
        let mut observation = new_observation(
            identity,
            "import",
            kind,
            spelling,
            range,
            node_range(node)?,
            identity_node,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        observation.alias = enclosing_variable_name(node, source)?;
        observation.attributes.push(if is_static {
            "static_specifier".to_owned()
        } else {
            "dynamic_specifier".to_owned()
        });
        observation.attributes.push("role:value".to_owned());
        observation
            .limitations
            .push("module_resolution_not_performed".to_owned());
        if !is_static {
            observation
                .limitations
                .push("dynamic_module_specifier_not_resolved".to_owned());
        }
        Ok(vec![observation])
    }

    fn commonjs_export(
        self,
        identity: IdentityContext<'_>,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let Some(left) = node.child_by_field_name("left") else {
            return Ok(Vec::new());
        };
        let spelling = text(left, source)?.to_owned();
        if !(spelling == "module.exports"
            || spelling.starts_with("module.exports.")
            || spelling.starts_with("module.exports[")
            || spelling.starts_with("exports.")
            || spelling.starts_with("exports["))
        {
            return Ok(Vec::new());
        }
        let range = node_range(left)?;
        let mut observation = new_observation(
            identity,
            "import",
            "commonjs_export",
            spelling.clone(),
            range,
            node_range(node)?,
            left,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        observation.alias = spelling
            .rsplit_once('.')
            .map(|(_, property)| property.to_owned());
        observation.attributes.push("role:value".to_owned());
        observation
            .limitations
            .push("commonjs_interop_syntax_only".to_owned());
        if spelling.contains('[') {
            observation
                .limitations
                .push("computed_export_name_not_resolved".to_owned());
        }
        Ok(vec![observation])
    }
}

impl SourceAdapter for EcmaAdapter {
    fn scope_kind(self, node: Node<'_>) -> &'static str {
        match node.kind() {
            "program" => "file",
            "class_body" => "class",
            "interface_body" => "interface",
            "internal_module" => "namespace",
            "arrow_function" => "arrow_function",
            "method_definition" | "method_signature" | "abstract_method_signature" => "method",
            "function_declaration"
            | "generator_function_declaration"
            | "function_expression"
            | "generator_function"
            | "function_signature" => "function",
            "statement_block" => "block",
            _ => "scope",
        }
    }

    fn scope_spelling(self, node: Node<'_>, source: &[u8]) -> Result<String, LanguageError> {
        match node.kind() {
            "program" => Ok("<file>".to_owned()),
            "class_body" | "interface_body" => node
                .parent()
                .and_then(|parent| parent.child_by_field_name("name"))
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| format!("<{}>", node.kind()))),
            "statement_block" => Ok("<block>".to_owned()),
            "arrow_function" => enclosing_variable_name(node, source)
                .map(|name| name.unwrap_or_else(|| "<arrow>".to_owned())),
            "internal_module"
            | "method_definition"
            | "method_signature"
            | "abstract_method_signature"
            | "function_declaration"
            | "generator_function_declaration"
            | "function_expression"
            | "generator_function"
            | "function_signature" => node
                .child_by_field_name("name")
                .map(|name| text(name, source).map(str::to_owned))
                .transpose()
                .map(|name| name.unwrap_or_else(|| format!("<{}>", node.kind()))),
            _ => Ok(format!("<{}>", node.kind())),
        }
    }

    fn declarations(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let identity = IdentityContext::new(language, relative_path);
        if capture_kind == "parameter" {
            return binding_name_nodes(node)
                .into_iter()
                .map(|name| {
                    self.declaration_from_name(identity, "parameter", name, node, source, scopes)
                })
                .collect();
        }
        if capture_kind == "binding" {
            let Some(pattern) = node.child_by_field_name("name") else {
                return Ok(Vec::new());
            };
            let value_kind = node.child_by_field_name("value").map(|value| value.kind());
            let kind = match value_kind {
                Some("arrow_function" | "function_expression" | "generator_function") => {
                    "function_binding"
                }
                _ => "variable",
            };
            return binding_name_nodes(pattern)
                .into_iter()
                .map(|name| self.declaration_from_name(identity, kind, name, node, source, scopes))
                .collect();
        }
        let Some(name) = node.child_by_field_name("name") else {
            return Ok(Vec::new());
        };
        let kind = if capture_kind == "method" && text(name, source)? == "constructor" {
            "constructor"
        } else {
            capture_kind
        };
        Ok(vec![self.declaration_from_name(
            identity, kind, name, node, source, scopes,
        )?])
    }

    fn imports(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Vec<SyntaxObservation>, LanguageError> {
        let identity = IdentityContext::new(language, relative_path);
        match capture_kind {
            "esm" => self.esm_imports(identity, node, source, scopes),
            "esm_export" => self.esm_exports(identity, node, source, scopes),
            "module_call" => self.module_call_import(identity, node, source, scopes),
            "commonjs_export" => self.commonjs_export(identity, node, source, scopes),
            _ => Ok(Vec::new()),
        }
    }

    fn call(
        self,
        language: LanguageId,
        relative_path: &Path,
        capture_kind: &str,
        node: Node<'_>,
        source: &[u8],
        scopes: &[SyntaxObservation],
    ) -> Result<Option<SyntaxObservation>, LanguageError> {
        let target = if capture_kind == "constructor" {
            node.child_by_field_name("constructor")
        } else {
            node.child_by_field_name("function")
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let target_text = text(target, source)?.to_owned();
        let kind = if capture_kind == "constructor" {
            "constructor"
        } else if target.kind() == "import" {
            "dynamic_import"
        } else if target_text == "require" {
            "require"
        } else if matches!(target.kind(), "member_expression" | "subscript_expression") {
            "method"
        } else {
            "function"
        };
        let range = node_range(target)?;
        let mut observation = new_observation(
            IdentityContext::new(language, relative_path),
            "call",
            kind,
            target_text,
            range,
            node_range(node)?,
            node,
        );
        observation.scope_id = containing_scope_id(range, scopes, None);
        observation.container = self.container(node, source)?;
        observation.resolution = ResolutionCategory::Unresolved;
        let call_text = text(node, source)?;
        if call_text.contains("?.") {
            observation.attributes.push("optional_chain".to_owned());
        }
        if nearest_ancestor(node, &["await_expression"]).is_some() {
            observation.attributes.push("awaited".to_owned());
        }
        if target.kind() == "subscript_expression" {
            observation.attributes.push("computed_target".to_owned());
            observation
                .limitations
                .push("dynamic_property_target_not_resolved".to_owned());
        } else if kind == "dynamic_import" || kind == "require" {
            let static_specifier = node
                .child_by_field_name("arguments")
                .and_then(|arguments| arguments.named_child(0))
                .is_some_and(|argument| argument.kind() == "string");
            if !static_specifier {
                observation
                    .limitations
                    .push("dynamic_module_specifier_not_resolved".to_owned());
            }
            observation
                .limitations
                .push("module_resolution_not_performed".to_owned());
        } else if kind == "method" {
            observation
                .limitations
                .push("dispatch_target_not_resolved".to_owned());
        } else {
            observation
                .limitations
                .push("call_target_not_resolved".to_owned());
        }
        Ok(Some(observation))
    }

    fn exclude_reference(self, capture_kind: &str, node: Node<'_>, _source: &[u8]) -> bool {
        if capture_kind == "identifier" && nearest_ancestor(node, &["decorator"]).is_some() {
            return true;
        }
        capture_kind == "identifier"
            && self.jsx
            && nearest_ancestor(
                node,
                &[
                    "jsx_opening_element",
                    "jsx_self_closing_element",
                    "jsx_closing_element",
                ],
            )
            .is_some()
    }

    fn decorate_reference(
        self,
        capture_kind: &str,
        _node: Node<'_>,
        _source: &[u8],
        observation: &mut SyntaxObservation,
    ) -> Result<(), LanguageError> {
        match capture_kind {
            "type" => observation.attributes.push("role:type".to_owned()),
            "jsx_component" => {
                observation.attributes.push(
                    if observation
                        .spelling
                        .chars()
                        .next()
                        .is_some_and(char::is_uppercase)
                        || observation.spelling.contains('.')
                    {
                        "jsx_component_syntax".to_owned()
                    } else {
                        "jsx_intrinsic_syntax".to_owned()
                    },
                );
                observation
                    .limitations
                    .push("jsx_tag_not_a_call_edge".to_owned());
            }
            "computed_property" => {
                observation
                    .limitations
                    .push("computed_property_name_not_resolved".to_owned());
            }
            "decorator" => {
                observation.attributes.push("role:decorator".to_owned());
                observation
                    .limitations
                    .push("decorator_target_not_resolved".to_owned());
            }
            _ => observation.attributes.push("role:value".to_owned()),
        }
        Ok(())
    }

    fn diagnostics(self, _conditions: &[SyntaxObservation]) -> Vec<Diagnostic> {
        vec![Diagnostic {
            code: if self.typescript {
                "typescript_source_only_limits".to_owned()
            } else {
                "javascript_source_only_limits".to_owned()
            },
            message: "ECMAScript-family extraction is syntax-only: configuration and source are not executed, package/module resolution and path aliases are not evaluated, and call/JSX targets are not resolved".to_owned(),
            severity: DiagnosticSeverity::Information,
            range: None,
        }]
    }
}

fn immediate_named_children<'tree>(node: Node<'tree>, kind: &str) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect()
}

fn attached_decorators(node: Node<'_>) -> Vec<Node<'_>> {
    let mut decorators = immediate_named_children(node, "decorator");
    let mut sibling = node.prev_named_sibling();
    while let Some(previous) = sibling {
        if previous.kind() != "decorator" {
            break;
        }
        decorators.push(previous);
        sibling = previous.prev_named_sibling();
    }
    if let Some(parent) = node.parent()
        && parent.kind() == "export_statement"
    {
        decorators.extend(immediate_named_children(parent, "decorator"));
    }
    decorators.sort_by_key(Node::start_byte);
    decorators.dedup_by_key(|decorator| decorator.id());
    decorators
}

fn first_named_descendant<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    named_descendants(node, kinds).into_iter().next()
}

fn binding_name_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    fn visit<'tree>(node: Node<'tree>, output: &mut Vec<Node<'tree>>) {
        match node.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => output.push(node),
            "required_parameter" | "optional_parameter" => {
                if let Some(pattern) = node.child_by_field_name("pattern") {
                    visit(pattern, output);
                }
            }
            "pair_pattern" => {
                if let Some(value) = node.child_by_field_name("value") {
                    visit(value, output);
                }
            }
            "assignment_pattern" => {
                if let Some(left) = node.child_by_field_name("left") {
                    visit(left, output);
                }
            }
            "rest_pattern" => {
                if let Some(argument) = node.child_by_field_name("argument") {
                    visit(argument, output);
                }
            }
            "formal_parameters" | "object_pattern" | "array_pattern" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    visit(child, output);
                }
            }
            _ => {}
        }
    }
    let mut output = Vec::new();
    visit(node, &mut output);
    output
}

fn module_source<'tree>(
    node: Node<'tree>,
    source: &[u8],
) -> Result<Option<(String, Node<'tree>)>, LanguageError> {
    let source_node = node.child_by_field_name("source");
    source_node
        .map(|source_node| text(source_node, source).map(|value| (unquote(value), source_node)))
        .transpose()
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && matches!(
            (bytes.first(), bytes.last()),
            (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"'))
        )
    {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

fn enclosing_variable_name(node: Node<'_>, source: &[u8]) -> Result<Option<String>, LanguageError> {
    let Some(declarator) = nearest_ancestor(node, &["variable_declarator"]) else {
        return Ok(None);
    };
    let Some(pattern) = declarator.child_by_field_name("name") else {
        return Ok(None);
    };
    binding_name_nodes(pattern)
        .into_iter()
        .next()
        .map(|name| text(name, source).map(str::to_owned))
        .transpose()
}
