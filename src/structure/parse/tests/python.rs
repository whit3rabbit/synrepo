use super::super::*;
use std::path::Path;

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
