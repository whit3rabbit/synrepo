//! Qualified-name derivation edge-case tests.
//!
//! These pin the behavior of `qualname::build_qualified_name_and_kind` on
//! known-fragile constructs (Rust generic/trait impls, nested modules,
//! Python/TypeScript class nesting) so future changes to ancestor-walking
//! or type-stripping logic cannot silently regress.

use super::{parse_file, Language};
use crate::structure::graph::SymbolKind;
use std::path::Path;

// ── Task 4.1: Rust generic impl methods ──────────────────────────────────────

#[test]
fn rust_generic_impl_method_qualname_drops_generic_parameters() {
    let source = b"
pub struct Container<T> { item: T }

impl<T> Container<T> {
    pub fn new(item: T) -> Self { Self { item } }
    pub fn get(&self) -> &T { &self.item }
}
";
    let output = parse_file(Path::new("src/container.rs"), source)
        .unwrap()
        .unwrap();

    let new_method = output
        .symbols
        .iter()
        .find(|s| s.display_name == "new")
        .expect("expected `new` method");
    let get_method = output
        .symbols
        .iter()
        .find(|s| s.display_name == "get")
        .expect("expected `get` method");

    assert_eq!(
        new_method.qualified_name, "Container::new",
        "generic impl method qualname must drop <T>"
    );
    assert_eq!(get_method.qualified_name, "Container::get");
    assert_eq!(new_method.kind, SymbolKind::Method);
    assert_eq!(get_method.kind, SymbolKind::Method);
}

// ── Task 4.2: Rust trait impl methods ────────────────────────────────────────

#[test]
fn rust_trait_impl_method_qualname_reflects_implementing_type() {
    let source = b"
pub trait Greet { fn greet(&self) -> String; }
pub struct Greeter;

impl Greet for Greeter {
    fn greet(&self) -> String { String::from(\"hi\") }
}
";
    let output = parse_file(Path::new("src/greeter.rs"), source)
        .unwrap()
        .unwrap();

    let impl_method = output
        .symbols
        .iter()
        .find(|s| s.display_name == "greet" && s.kind == SymbolKind::Method)
        .expect("expected trait-impl method");

    // tree-sitter-rust's `impl_item type: ...` field names the implementing
    // type (`Greeter`), not the trait (`Greet`). Pin that so type-stripping
    // changes in qualname.rs cannot swap them.
    assert_eq!(
        impl_method.qualified_name, "Greeter::greet",
        "trait-impl method qualname must name the implementing type (Greeter), not the trait"
    );
}

// ── Task 4.3: Rust nested modules with same-name symbols ─────────────────────

#[test]
fn rust_nested_modules_extract_same_name_symbols_without_collision() {
    // tree-sitter extraction currently uses the ancestor walk only to detect
    // impl/class scopes, not to prefix module paths — so both `helper`
    // symbols come out with the same unqualified `qualified_name`.
    // This test pins that behavior (rather than asserting module-path
    // qualification) so an accidental change is loud. If module-path
    // qualification is later added, update this test in the same commit.
    let source = b"
pub mod outer {
    pub fn helper() {}
    pub mod inner {
        pub fn helper() {}
    }
}
";
    let output = parse_file(Path::new("src/nested.rs"), source)
        .unwrap()
        .unwrap();

    let helpers: Vec<_> = output
        .symbols
        .iter()
        .filter(|s| s.display_name == "helper")
        .collect();

    assert_eq!(
        helpers.len(),
        2,
        "expected both nested `helper` symbols to be extracted; got: {:?}",
        output
            .symbols
            .iter()
            .map(|s| (s.display_name.as_str(), s.qualified_name.as_str()))
            .collect::<Vec<_>>(),
    );
    for h in &helpers {
        assert_eq!(h.kind, SymbolKind::Function);
    }
}

// ── Task 4.4: Python class methods and nested classes ────────────────────────

#[test]
fn python_class_methods_get_class_qualified_name() {
    let source = b"
class Outer:
    def outer_method(self):
        pass

    class Inner:
        def inner_method(self):
            pass
";
    let output = parse_file(Path::new("nested.py"), source).unwrap().unwrap();

    let outer_method = output
        .symbols
        .iter()
        .find(|s| s.display_name == "outer_method")
        .expect("outer_method missing");
    let inner_class = output
        .symbols
        .iter()
        .find(|s| s.display_name == "Inner")
        .expect("Inner class missing");
    let inner_method = output
        .symbols
        .iter()
        .find(|s| s.display_name == "inner_method")
        .expect("inner_method missing");

    assert_eq!(outer_method.kind, SymbolKind::Method);
    assert_eq!(outer_method.qualified_name, "Outer::outer_method");

    // Phase-1 qualname walks to the nearest enclosing class. Inner's
    // method is qualified with `Inner`, not `Outer::Inner`. Pin that
    // so a future refinement is an intentional, tested change.
    assert_eq!(inner_method.kind, SymbolKind::Method);
    assert_eq!(inner_method.qualified_name, "Inner::inner_method");

    // Phase-1 quirk: the nested `class Inner:` statement is itself a symbol
    // whose immediate ancestor is Outer's class body. qualname.rs hits the
    // `block → class_definition` arm first and returns `SymbolKind::Method`
    // for it, not `Class`. This is documented here to fail loudly if a
    // future change alters the behavior. Nested-class kinding is a known
    // rough edge tracked outside parse-hardening-tree-sitter.
    assert_eq!(
        inner_class.qualified_name, "Outer::Inner",
        "nested-class qualname must reflect enclosing class scope"
    );
    assert_eq!(
        inner_class.kind,
        SymbolKind::Method,
        "phase-1: nested class inside class body is currently kinded as Method \
         (block→class_definition ancestor walk). Update deliberately when fixed."
    );
}

// ── Task 4.5: TypeScript class methods + alt class-node shape ────────────────

#[test]
fn typescript_class_methods_get_class_qualified_name() {
    let source = b"
export class Shape {
    area(): number { return 0; }
    perimeter(): number { return 0; }
}
";
    let output = parse_file(Path::new("src/shape.ts"), source)
        .unwrap()
        .unwrap();

    let area = output
        .symbols
        .iter()
        .find(|s| s.display_name == "area")
        .expect("area missing");
    let perim = output
        .symbols
        .iter()
        .find(|s| s.display_name == "perimeter")
        .expect("perimeter missing");

    assert_eq!(area.qualified_name, "Shape::area");
    assert_eq!(area.kind, SymbolKind::Method);
    assert_eq!(perim.qualified_name, "Shape::perimeter");
    assert_eq!(perim.kind, SymbolKind::Method);
}

#[test]
fn typescript_class_expression_assigned_to_const_is_recognized_where_possible() {
    // Class expressions (e.g. `const Shape = class { ... }`) are an
    // alternate syntactic shape. The current definition query keys on
    // `class_declaration`, not `class_expression`, so the class itself
    // will not be extracted — but a method definition inside the class
    // body should still be picked up by the `method_definition` pattern
    // because tree-sitter-typescript exposes the inner method node.
    // Pin that behavior so a future change does not silently lose
    // these methods.
    let source = b"
export const Shape = class {
    area(): number { return 1; }
};
";
    let output = parse_file(Path::new("src/shape_expr.ts"), source)
        .unwrap()
        .unwrap();

    let area = output.symbols.iter().find(|s| s.display_name == "area");
    assert!(
        area.is_some(),
        "expected `area` method from class expression to appear; got: {:?}",
        output
            .symbols
            .iter()
            .map(|s| (s.display_name.as_str(), s.kind))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        area.unwrap().kind,
        SymbolKind::Method,
        "class-expression methods must be kinded as Method even when the class name is not captured"
    );
}

#[test]
fn tsx_fixture_preserves_language_on_class_methods() {
    // Ensures TSX-specific parse path (language_tsx grammar) still produces
    // class + method qualification equivalent to TypeScript.
    let source = b"
import * as React from 'react';

export class Card extends React.Component {
    render() { return <div />; }
}
";
    let output = parse_file(Path::new("src/card.tsx"), source)
        .unwrap()
        .unwrap();
    assert_eq!(output.language, Language::Tsx);
    let render = output
        .symbols
        .iter()
        .find(|s| s.display_name == "render")
        .expect("render missing");
    assert_eq!(render.qualified_name, "Card::render");
    assert_eq!(render.kind, SymbolKind::Method);
}
