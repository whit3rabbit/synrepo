use crate::structure::graph::SymbolKind;

/// Walk up the parent chain to determine qualified name and kind.
///
/// For Rust functions inside an `impl` block, the kind becomes `Method` and
/// the qualified name is prefixed with the impl type (for example `MyStruct::foo`).
/// For Python methods inside a `class_definition`, the same rule applies.
pub(super) fn build_qualified_name_and_kind(
    node: tree_sitter::Node,
    name: &str,
    source: &[u8],
    base_kind: SymbolKind,
) -> (String, SymbolKind) {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        match parent.kind() {
            "impl_item" => {
                if let Some(type_node) = parent.child_by_field_name("type") {
                    let type_bytes = &source[type_node.start_byte()..type_node.end_byte()];
                    if let Ok(type_name) = std::str::from_utf8(type_bytes) {
                        let base = type_name.split('<').next().unwrap_or(type_name).trim();
                        return (format!("{base}::{name}"), SymbolKind::Method);
                    }
                }
            }
            "block" => {
                if let Some(grandparent) = parent.parent() {
                    if grandparent.kind() == "class_definition" {
                        if let Some(class_name_node) = grandparent.child_by_field_name("name") {
                            let class_bytes =
                                &source[class_name_node.start_byte()..class_name_node.end_byte()];
                            if let Ok(class_name) = std::str::from_utf8(class_bytes) {
                                return (format!("{class_name}::{name}"), SymbolKind::Method);
                            }
                        }
                    }
                }
            }
            "class_body" => {
                if let Some(grandparent) = parent.parent() {
                    if matches!(grandparent.kind(), "class_declaration" | "class") {
                        if let Some(class_name_node) = grandparent.child_by_field_name("name") {
                            let class_bytes =
                                &source[class_name_node.start_byte()..class_name_node.end_byte()];
                            if let Ok(class_name) = std::str::from_utf8(class_bytes) {
                                return (format!("{class_name}::{name}"), SymbolKind::Method);
                            }
                        }
                    }
                }
            }
            "source_file" | "program" => break,
            _ => {}
        }
        ancestor = parent.parent();
    }

    (name.to_string(), base_kind)
}
