use super::*;
use std::path::Path;

// ── Go tests ──────────────────────────────────────────────────────────────────

#[test]
fn parse_go_file_extracts_function() {
    let source =
        b"package main\n\nfunc Greet(name string) string {\n\treturn \"Hello, \" + name\n}\n";
    let output = parse_file(Path::new("main.go"), source).unwrap().unwrap();
    assert_eq!(output.language, Language::Go);
    let names: Vec<&str> = output
        .symbols
        .iter()
        .map(|s| s.display_name.as_str())
        .collect();
    assert!(names.contains(&"Greet"), "expected Greet, got: {names:?}");
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "Greet")
        .unwrap();
    assert_eq!(sym.kind, SymbolKind::Function);
    let sig = sym.signature.as_deref().expect("expected signature");
    assert!(
        sig.contains("Greet"),
        "expected sig to contain 'Greet', got: {sig}"
    );
}

#[test]
fn parse_go_file_extracts_struct_type() {
    let source = b"package main\n\ntype Greeter struct {\n\tName string\n}\n";
    let output = parse_file(Path::new("types.go"), source).unwrap().unwrap();
    let names: Vec<&str> = output
        .symbols
        .iter()
        .map(|s| s.display_name.as_str())
        .collect();
    assert!(
        names.contains(&"Greeter"),
        "expected Greeter, got: {names:?}"
    );
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "Greeter")
        .unwrap();
    assert_eq!(sym.kind, SymbolKind::Class);
}

#[test]
fn parse_go_file_extracts_interface_type() {
    let source = b"package main\n\ntype Namer interface {\n\tGetName() string\n}\n";
    let output = parse_file(Path::new("iface.go"), source).unwrap().unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "Namer")
        .expect("expected Namer interface");
    assert_eq!(sym.kind, SymbolKind::Interface);
}

#[test]
fn parse_go_file_extracts_method() {
    let source =
        b"package main\n\ntype Greeter struct{ Name string }\n\nfunc (g *Greeter) GetName() string {\n\treturn g.Name\n}\n";
    let output = parse_file(Path::new("greeter.go"), source)
        .unwrap()
        .unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "GetName")
        .expect("expected GetName method");
    assert_eq!(sym.kind, SymbolKind::Method);
}

#[test]
fn parse_go_file_extracts_doc_comment() {
    let source =
        b"package main\n\n// Greet returns a greeting.\nfunc Greet(name string) string {\n\treturn name\n}\n";
    let output = parse_file(Path::new("main.go"), source).unwrap().unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "Greet")
        .expect("Greet not found");
    assert_eq!(
        sym.doc_comment.as_deref(),
        Some("Greet returns a greeting."),
        "expected Go doc comment"
    );
}

#[test]
fn parse_go_file_extracts_imports() {
    let source = b"package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc main() {}\n";
    let output = parse_file(Path::new("main.go"), source).unwrap().unwrap();
    let import_refs: Vec<&str> = output
        .import_refs
        .iter()
        .map(|r| r.module_ref.as_str())
        .collect();
    assert!(
        import_refs.iter().any(|r| r.contains("fmt")),
        "expected fmt import, got: {import_refs:?}"
    );
}

/// Grammar validation test: confirms minimum expected symbol count on the Go
/// fixture. Fails if the grammar is upgraded and queries break silently.
#[test]
fn go_fixture_grammar_validation() {
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/go/main.go"
    ))
    .expect("fixture not found");
    let output = parse_file(Path::new("main.go"), &fixture).unwrap().unwrap();

    let functions: Vec<_> = output
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .collect();
    let methods: Vec<_> = output
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method)
        .collect();
    let structs: Vec<_> = output
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Class)
        .collect();
    let interfaces: Vec<_> = output
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Interface)
        .collect();
    let constants: Vec<_> = output
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Constant)
        .collect();

    assert!(
        functions.len() >= 1,
        "expected at least 1 function, got {}",
        functions.len()
    );
    assert!(
        methods.len() >= 2,
        "expected at least 2 methods, got {}",
        methods.len()
    );
    assert!(
        structs.len() >= 1,
        "expected at least 1 struct, got {}",
        structs.len()
    );
    assert!(
        interfaces.len() >= 1,
        "expected at least 1 interface, got {}",
        interfaces.len()
    );
    assert!(
        constants.len() >= 1,
        "expected at least 1 constant, got {}",
        constants.len()
    );
    assert!(
        !output.import_refs.is_empty(),
        "expected at least 1 import ref"
    );
}

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
fn parse_file_returns_none_for_unsupported_extension() {
    assert!(parse_file(Path::new("config.yaml"), b"key: val")
        .unwrap()
        .is_none());
    assert!(parse_file(Path::new("README.md"), b"# hi")
        .unwrap()
        .is_none());
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
fn parse_file_extracts_python_functions_and_classes() {
    let source = b"
def greet(name):
    return f'Hello, {name}'

class Greeter:
    def greet(self):
        return 'hi'
";
    let output = parse_file(Path::new("app.py"), source).unwrap().unwrap();

    let names: Vec<&str> = output
        .symbols
        .iter()
        .map(|symbol| symbol.display_name.as_str())
        .collect();
    assert!(names.contains(&"greet"), "expected greet in {names:?}");
    assert!(names.contains(&"Greeter"), "expected Greeter in {names:?}");
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
fn parse_file_extracts_python_docstring_and_signature() {
    let source = b"def greet(name):\n    \"\"\"Greet someone.\"\"\"\n    return f'Hello, {name}'\n";
    let output = parse_file(Path::new("app.py"), source).unwrap().unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "greet")
        .expect("greet not found");
    assert_eq!(
        sym.doc_comment.as_deref(),
        Some("Greet someone."),
        "expected Python docstring"
    );
    let sig = sym.signature.as_deref().expect("expected Python signature");
    assert!(
        sig.starts_with("def greet"),
        "expected sig to start with 'def greet', got: {sig}"
    );
}

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

#[test]
fn parse_file_python_no_doc_yields_none_doc_comment_but_some_signature() {
    let source = b"def no_doc():\n    pass\n";
    let output = parse_file(Path::new("app.py"), source).unwrap().unwrap();
    let sym = output
        .symbols
        .iter()
        .find(|s| s.display_name == "no_doc")
        .unwrap();
    assert!(sym.doc_comment.is_none(), "Python: expected no doc_comment");
    assert!(sym.signature.is_some(), "Python: expected Some(signature)");
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
