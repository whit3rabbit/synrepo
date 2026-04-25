//! Visibility extraction for symbols.
//!
//! Extracts visibility based on language-specific rules:
//! - Rust: `pub`, `pub(crate)`, etc.
//! - Python: underscore prefix convention.
//! - TypeScript/TSX: `export` statement wrapper.
//! - Go: identifier capitalization.

use tree_sitter::Node;

use crate::structure::graph::Visibility;
use crate::structure::parse::Language;

/// Extract visibility from a parsed symbol node.
pub(super) fn extract_visibility(
    item_node: Node,
    source: &[u8],
    language: Language,
    display_name: &str,
) -> Visibility {
    match language {
        Language::Rust => extract_rust_visibility(item_node, source),
        Language::Python => extract_python_visibility(display_name),
        Language::TypeScript | Language::Tsx => extract_typescript_visibility(item_node, source),
        Language::Go => extract_go_visibility(display_name),
    }
}

/// Rust: inspect visibility_modifier child node.
fn extract_rust_visibility(item_node: Node, source: &[u8]) -> Visibility {
    // Try to find a visibility_modifier child node.
    let mut cursor = item_node.walk();
    for child in item_node.children(&mut cursor) {
        let kind = child.kind();
        if kind == "visibility_modifier" {
            let text = child.utf8_text(source).unwrap_or("");
            if text == "pub" {
                return Visibility::Public;
            } else if text.starts_with("pub(") {
                // pub(crate), pub(super), pub(in path)
                return Visibility::Crate;
            }
            return Visibility::Private;
        }
    }
    // No visibility modifier means private.
    Visibility::Private
}

/// Python: underscore prefix convention.
/// Dunders (__name__) are public protocol, single underscore is private.
fn extract_python_visibility(display_name: &str) -> Visibility {
    if display_name.starts_with("__") && display_name.ends_with("__") {
        // Dunder names are public protocol.
        Visibility::Public
    } else if display_name.starts_with('_') {
        // Single underscore prefix is private convention.
        Visibility::Private
    } else {
        Visibility::Public
    }
}

/// TypeScript/TSX: class members read `accessibility_modifier`; everything
/// else falls back to the export-statement parent check.
///
/// `protected` maps to a dedicated `Visibility::Protected` variant: it is
/// accessible across files (in subclasses) but is not public API.
fn extract_typescript_visibility(item_node: Node, source: &[u8]) -> Visibility {
    // Class members: `method_definition` and `public_field_definition` carry
    // an optional `accessibility_modifier` child. `abstract_method_signature`
    // can carry one too inside an abstract class. Defensive on all three.
    if matches!(
        item_node.kind(),
        "method_definition" | "public_field_definition" | "abstract_method_signature"
    ) {
        if let Some(modifier) = class_member_accessibility(item_node, source) {
            return modifier;
        }
        // No modifier on a class member → TS default is public.
        return Visibility::Public;
    }

    // Free declarations: parent is `export_statement` → Public; otherwise Private.
    if let Some(parent) = item_node.parent() {
        if parent.kind() == "export_statement" {
            return Visibility::Public;
        }
    }
    Visibility::Public
}

fn class_member_accessibility(item_node: Node, source: &[u8]) -> Option<Visibility> {
    let mut cursor = item_node.walk();
    for child in item_node.children(&mut cursor) {
        if child.kind() == "accessibility_modifier" {
            return Some(match child.utf8_text(source).unwrap_or("") {
                "private" => Visibility::Private,
                "protected" => Visibility::Protected,
                _ => Visibility::Public,
            });
        }
    }
    None
}

/// Go: uppercase first character means exported.
fn extract_go_visibility(display_name: &str) -> Visibility {
    let first_char = display_name.chars().next();
    match first_char {
        Some(c) if c.is_uppercase() => Visibility::Public,
        _ => Visibility::Private,
    }
}
