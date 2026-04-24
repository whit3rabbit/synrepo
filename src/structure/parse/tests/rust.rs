use super::super::*;
use std::path::Path;

#[test]
fn parse_file_extracts_rust_top_level_definitions() {
    let source = b"
/// Greets a user.
pub fn greet(name: &str) -> String {
    format!(\"Hello, {name}!\")
}

pub struct Greeter {
    name: String,
}

pub trait Greetable {
    fn greet(&self) -> String;
}

pub enum Status {
    Active,
    Inactive,
}

pub type Name = String;

pub mod helpers {}

pub const MAX: usize = 100;
";
    let output = parse_file(Path::new("src/lib.rs"), source)
        .unwrap()
        .unwrap();

    assert_eq!(output.language, Language::Rust);
    let names: Vec<&str> = output
        .symbols
        .iter()
        .map(|symbol| symbol.display_name.as_str())
        .collect();
    assert!(names.contains(&"greet"), "expected greet, got: {names:?}");
    assert!(
        names.contains(&"Greeter"),
        "expected Greeter, got: {names:?}"
    );
    assert!(
        names.contains(&"Greetable"),
        "expected Greetable, got: {names:?}"
    );
    assert!(names.contains(&"Status"), "expected Status, got: {names:?}");
    assert!(names.contains(&"Name"), "expected Name, got: {names:?}");
    assert!(
        names.contains(&"helpers"),
        "expected helpers, got: {names:?}"
    );
    assert!(names.contains(&"MAX"), "expected MAX, got: {names:?}");
}

#[test]
fn parse_file_qualifies_rust_impl_methods() {
    let source = b"
pub struct Calculator {}

impl Calculator {
    /// Adds two numbers.
    pub fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}
";
    let output = parse_file(Path::new("src/calc.rs"), source)
        .unwrap()
        .unwrap();

    let method = output
        .symbols
        .iter()
        .find(|symbol| symbol.display_name == "add")
        .expect("add method not found");

    assert_eq!(method.kind, SymbolKind::Method);
    assert_eq!(method.qualified_name, "Calculator::add");
}

#[test]
fn parse_file_returns_empty_symbols_for_empty_rust_file() {
    let output = parse_file(Path::new("src/empty.rs"), b"").unwrap().unwrap();
    assert!(output.symbols.is_empty());
}

#[test]
fn parse_file_body_hash_is_stable_for_same_content() {
    let source = b"pub fn foo() {}";
    let first = parse_file(Path::new("a.rs"), source).unwrap().unwrap();
    let second = parse_file(Path::new("b.rs"), source).unwrap().unwrap();
    assert_eq!(first.symbols[0].body_hash, second.symbols[0].body_hash);
}

#[test]
fn parse_file_extracts_rust_doc_comment_and_signature() {
    let source = b"/// Greet a user by name.\npub fn greet(name: &str) -> String {\n    format!(\"Hello!\")\n}\n";
    let output = parse_file(Path::new("src/lib.rs"), source)
        .unwrap()
        .unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "greet")
        .expect("greet not found");
    assert_eq!(
        sym.doc_comment.as_deref(),
        Some("Greet a user by name."),
        "expected Rust doc_comment"
    );
    let sig = sym.signature.as_deref().expect("expected Rust signature");
    assert!(
        sig.starts_with("pub fn greet"),
        "expected sig to start with 'pub fn greet', got: {sig}"
    );
}

#[test]
fn parse_file_rust_no_doc_yields_none_doc_comment_but_some_signature() {
    let source = b"pub fn no_doc() {}\n";
    let output = parse_file(Path::new("src/lib.rs"), source)
        .unwrap()
        .unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "no_doc")
        .unwrap();
    assert!(sym.doc_comment.is_none(), "Rust: expected no doc_comment");
    assert!(sym.signature.is_some(), "Rust: expected Some(signature)");
}
