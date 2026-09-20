#[test]
fn core_manifest_has_no_protocol_storage_or_parser_dependency() {
    let manifest = include_str!("../../ca-core/Cargo.toml");
    for forbidden in ["rmcp", "rusqlite", "tree-sitter", "tree_sitter"] {
        assert!(
            !manifest.contains(forbidden),
            "ca-core must not depend on {forbidden}"
        );
    }
}

#[test]
fn parser_dependencies_are_isolated_to_the_language_crate() {
    let language_manifest = include_str!("../../ca-languages/Cargo.toml");
    let storage_manifest = include_str!("../../ca-storage/Cargo.toml");
    let engine_manifest = include_str!("../../ca-engine/Cargo.toml");
    assert!(language_manifest.contains("tree-sitter.workspace = true"));
    assert!(storage_manifest.contains("rusqlite"));
    assert!(!storage_manifest.contains("tree-sitter"));
    assert!(!engine_manifest.contains("tree-sitter"));
}
