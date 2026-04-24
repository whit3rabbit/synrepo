use super::super::*;
use std::path::Path;

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
        !functions.is_empty(),
        "expected at least 1 function, got {}",
        functions.len()
    );
    assert!(
        methods.len() >= 2,
        "expected at least 2 methods, got {}",
        methods.len()
    );
    assert!(
        !structs.is_empty(),
        "expected at least 1 struct, got {}",
        structs.len()
    );
    assert!(
        !interfaces.is_empty(),
        "expected at least 1 interface, got {}",
        interfaces.len()
    );
    assert!(
        !constants.is_empty(),
        "expected at least 1 constant, got {}",
        constants.len()
    );
    assert!(
        !output.import_refs.is_empty(),
        "expected at least 1 import ref"
    );
}
