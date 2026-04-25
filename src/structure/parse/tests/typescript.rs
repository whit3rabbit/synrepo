use super::super::*;
use crate::structure::graph::Visibility;
use std::path::Path;

#[test]
fn parse_file_extracts_typescript_jsdoc_and_signature() {
    let source =
        b"/** Returns a greeting. */\nfunction greet(name: string): string {\n    return `Hi`;\n}\n";
    let output = parse_file(Path::new("src/greet.ts"), source)
        .unwrap()
        .unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "greet")
        .expect("greet not found");
    assert_eq!(
        sym.doc_comment.as_deref(),
        Some("Returns a greeting."),
        "expected TS JSDoc"
    );
    let sig = sym.signature.as_deref().expect("expected TS signature");
    assert!(
        sig.starts_with("function greet"),
        "expected sig to start with 'function greet', got: {sig}"
    );
}

#[test]
fn parse_file_typescript_no_doc_yields_none_doc_comment_but_some_signature() {
    let source = b"function no_doc(): void {}\n";
    let output = parse_file(Path::new("src/lib.ts"), source)
        .unwrap()
        .unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "no_doc")
        .unwrap();
    assert!(
        sym.doc_comment.is_none(),
        "TypeScript: expected no doc_comment"
    );
    assert!(
        sym.signature.is_some(),
        "TypeScript: expected Some(signature)"
    );
}

/// Helper: parse a TS fixture and return the `Visibility` of the named symbol.
/// Each caller prefixes a unique comment so fixtures share no bytes (FileNodeId hash invariant).
fn visibility_of(path: &str, source: &[u8], target: &str) -> Visibility {
    let output = parse_file(Path::new(path), source)
        .unwrap()
        .expect("parse_file returned None");
    output
        .symbols
        .iter()
        .find(|s| s.display_name == target)
        .unwrap_or_else(|| panic!("symbol `{target}` not found in {path}"))
        .visibility
}

#[test]
fn typescript_class_method_public_modifier_is_public() {
    // Case 1/5: explicit `public` on a class method.
    let source = b"// ts-vis-case-1\nclass A { public foo(): void {} }\n";
    assert_eq!(
        visibility_of("src/cls1.ts", source, "foo"),
        Visibility::Public
    );
}

#[test]
fn typescript_class_method_private_modifier_is_private() {
    // Case 2/5: explicit `private` on a class method.
    let source = b"// ts-vis-case-2\nclass A { private foo(): void {} }\n";
    assert_eq!(
        visibility_of("src/cls2.ts", source, "foo"),
        Visibility::Private
    );
}

#[test]
fn typescript_class_method_protected_modifier_is_protected() {
    // Case 3/5: `protected` maps to its own Visibility variant.
    let source = b"// ts-vis-case-3\nclass A { protected foo(): void {} }\n";
    assert_eq!(
        visibility_of("src/cls3.ts", source, "foo"),
        Visibility::Protected
    );
}

#[test]
fn typescript_class_method_without_modifier_defaults_to_public() {
    // Case 4/5: no modifier on a class method → TS default `public`.
    let source = b"// ts-vis-case-4\nclass A { foo(): void {} }\n";
    assert_eq!(
        visibility_of("src/cls4.ts", source, "foo"),
        Visibility::Public
    );
}

#[test]
fn typescript_top_level_export_function_remains_public() {
    // Case 5/5: top-level `export function` path (not a class member) still Public.
    let source = b"// ts-vis-case-5\nexport function bar(): void {}\n";
    assert_eq!(
        visibility_of("src/top5.ts", source, "bar"),
        Visibility::Public
    );
}
