use crate::structure::graph::SymbolKind;

/// Walk up the parent chain to determine qualified name and kind.
///
/// For Rust functions inside an `impl` block, the kind becomes `Method` and
/// the qualified name is prefixed with the impl type (for example
/// `MyStruct::foo`). For Python methods inside nested `class_definition`
/// blocks, the qualname joins every enclosing class outer-first
/// (`Outer::Inner::method`). A nested class itself keeps `SymbolKind::Class`
/// and is qualified by its enclosing classes; only function-kinded nodes are
/// coerced to `Method`.
pub(super) fn build_qualified_name_and_kind(
    node: tree_sitter::Node,
    name: &str,
    source: &[u8],
    base_kind: SymbolKind,
) -> (String, SymbolKind) {
    // Innermost enclosing class names first; reversed for outer-first join
    // when building the qualified name.
    let mut class_chain: Vec<String> = Vec::new();
    let mut impl_type: Option<String> = None;

    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        match parent.kind() {
            "impl_item" => {
                if let Some(type_node) = parent.child_by_field_name("type") {
                    let type_bytes = &source[type_node.start_byte()..type_node.end_byte()];
                    if let Ok(type_name) = std::str::from_utf8(type_bytes) {
                        let base = type_name.split('<').next().unwrap_or(type_name).trim();
                        impl_type = Some(base.to_string());
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
                                class_chain.push(class_name.to_string());
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
                                class_chain.push(class_name.to_string());
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

    // Rust impl blocks take precedence: an impl-scoped function is always a
    // method on the impl type, regardless of any enclosing class-like nodes.
    if let Some(impl_type) = impl_type {
        return (format!("{impl_type}::{name}"), SymbolKind::Method);
    }

    if !class_chain.is_empty() {
        class_chain.reverse();
        let qname = format!("{}::{}", class_chain.join("::"), name);
        // Only coerce Function → Method. Container kinds (Class, Trait,
        // Interface, Type, TypeDef, Module) preserve their identity; only
        // their qualname picks up the enclosing-class prefix.
        let kind = if base_kind == SymbolKind::Function {
            SymbolKind::Method
        } else {
            base_kind
        };
        return (qname, kind);
    }

    (name.to_string(), base_kind)
}
