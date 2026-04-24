use super::super::*;
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
