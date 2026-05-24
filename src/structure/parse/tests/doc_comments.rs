use super::super::*;
use std::path::Path;

fn doc_for(path: &str, source: &str, symbol: &str) -> Option<String> {
    let output = parse_file(Path::new(path), source.as_bytes())
        .unwrap()
        .unwrap_or_else(|| panic!("parse_file returned None for {path}"));
    output
        .symbols
        .iter()
        .find(|item| item.display_name == symbol)
        .unwrap_or_else(|| panic!("symbol `{symbol}` not found in {path}"))
        .doc_comment
        .clone()
}

#[test]
fn javascript_jsdoc_before_exported_arrow_function_is_doc_comment() {
    let source = "/** Greets caller. */\nexport const greet = (name) => `Hi ${name}`;\n";

    assert_eq!(
        doc_for("src/greet.js", source, "greet").as_deref(),
        Some("Greets caller.")
    );
}

#[test]
fn javascript_plain_line_comment_is_not_doc_comment() {
    let source = "// Greets caller.\nfunction greet() {}\n";

    assert_eq!(doc_for("src/greet.js", source, "greet"), None);
}

#[test]
fn java_javadoc_is_doc_comment() {
    let source = "/** Greeter type. */\npublic class Greeter {}\n";

    assert_eq!(
        doc_for("src/Greeter.java", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn java_javadoc_before_method_is_doc_comment() {
    let source =
        "public class Greeter {\n/** Greets caller. */\npublic String greet() { return \"\"; }\n}\n";

    assert_eq!(
        doc_for("src/Greeter.java", source, "greet").as_deref(),
        Some("Greets caller.")
    );
}

#[test]
fn kotlin_kdoc_is_doc_comment() {
    let source = "/** Greeter type. */\nclass Greeter {}\n";

    assert_eq!(
        doc_for("src/Greeter.kt", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn csharp_xml_doc_comment_is_doc_comment() {
    let source = "/// Greeter type.\npublic class Greeter {}\n";

    assert_eq!(
        doc_for("src/Greeter.cs", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn php_docblock_is_doc_comment() {
    let source = "<?php\n/** Greeter type. */\nclass Greeter {}\n";

    assert_eq!(
        doc_for("src/Greeter.php", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn ruby_hash_comment_is_doc_comment() {
    let source = "# Greeter type.\nclass Greeter\nend\n";

    assert_eq!(
        doc_for("src/greeter.rb", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn swift_doc_comment_is_doc_comment() {
    let source = "/// Greeter type.\nclass Greeter {}\n";

    assert_eq!(
        doc_for("src/Greeter.swift", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn c_doxygen_comment_is_doc_comment() {
    let source = "/** Greets caller. */\nvoid greet(void) {}\n";

    assert_eq!(
        doc_for("src/greet.c", source, "greet").as_deref(),
        Some("Greets caller.")
    );
}

#[test]
fn cpp_doxygen_line_comment_is_doc_comment() {
    let source = "/// Greeter type.\nclass Greeter {};\n";

    assert_eq!(
        doc_for("src/greeter.cpp", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn dart_doc_comment_is_doc_comment() {
    let source = "/// Greeter type.\nclass Greeter {}\n";

    assert_eq!(
        doc_for("lib/greeter.dart", source, "Greeter").as_deref(),
        Some("Greeter type.")
    );
}

#[test]
fn dart_doc_comment_before_function_signature_is_doc_comment() {
    let source = "/// Greets caller.\nString greet(String name) => name;\n";

    assert_eq!(
        doc_for("lib/greeter.dart", source, "greet").as_deref(),
        Some("Greets caller.")
    );
}
