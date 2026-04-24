use super::*;
use crate::surface::card::compiler::test_support::bootstrap;
use crate::surface::card::compiler::{CardCompiler, GraphCardCompiler};
use std::fs;
use tempfile::tempdir;

// Fixture: a directory with two Rust files, mix of pub and private symbols.
fn write_auth_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("src/auth")).unwrap();
    fs::write(
        root.join("src/auth/mod.rs"),
        "pub fn authenticate(user: &str) -> bool { true }\n\
         pub(crate) fn internal_check() {}\n\
         fn private_helper() {}\n\
         pub struct Token { pub value: String }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/auth/session.rs"),
        "pub fn create_session() -> u64 { 0 }\n",
    )
    .unwrap();
}

#[test]
fn public_api_card_tiny_returns_count_only() {
    let repo = tempdir().unwrap();
    write_auth_fixture(repo.path());
    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

    let card = compiler.public_api_card("src/auth", Budget::Tiny).unwrap();

    // Tiny: symbols not materialised, but count is populated.
    assert!(card.public_symbol_count > 0, "expected some public symbols");
    assert!(
        card.public_symbols.is_empty(),
        "Tiny must not materialise symbols"
    );
    assert!(card.public_entry_points.is_empty());
    assert!(card.recent_api_changes.is_empty());
    assert!(card.path.ends_with('/'));
}

#[test]
fn public_api_card_normal_excludes_private() {
    let repo = tempdir().unwrap();
    write_auth_fixture(repo.path());
    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

    let card = compiler
        .public_api_card("src/auth", Budget::Normal)
        .unwrap();

    // `private_helper` must not appear.
    let names: Vec<&str> = card
        .public_symbols
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        !names.contains(&"private_helper"),
        "private_helper must be excluded; got: {names:?}"
    );

    // All materialised entries have visibility Public or Crate (filtered in compiler).
    // The test above verifies private_helper is excluded.

    // recent_api_changes empty at Normal.
    assert!(card.recent_api_changes.is_empty());
}

#[test]
fn public_api_card_count_matches_normal_list_length() {
    let repo = tempdir().unwrap();
    write_auth_fixture(repo.path());
    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

    let tiny = compiler.public_api_card("src/auth", Budget::Tiny).unwrap();
    let normal = compiler
        .public_api_card("src/auth", Budget::Normal)
        .unwrap();

    assert_eq!(
        tiny.public_symbol_count, normal.public_symbol_count,
        "count must agree across budgets"
    );
    assert_eq!(
        normal.public_symbols.len(),
        normal.public_symbol_count,
        "Normal list length must equal count"
    );
}

#[test]
fn public_api_card_empty_directory_returns_zero() {
    let repo = tempdir().unwrap();
    // Bootstrap needs at least one file.
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn noop() {}\n").unwrap();
    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

    let card = compiler
        .public_api_card("src/empty", Budget::Normal)
        .unwrap();

    assert_eq!(card.public_symbol_count, 0);
    assert!(card.public_symbols.is_empty());
    assert!(card.public_entry_points.is_empty());
}

#[test]
fn public_api_card_deep_no_git_has_empty_recent() {
    let repo = tempdir().unwrap();
    write_auth_fixture(repo.path());
    let graph = bootstrap(&repo);
    // No repo_root → git context absent → recent_api_changes must be empty.
    let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

    let card = compiler.public_api_card("src/auth", Budget::Deep).unwrap();

    assert!(
        card.recent_api_changes.is_empty(),
        "no git context → recent_api_changes must be empty"
    );
    // Symbols are still materialised at Deep.
    assert!(!card.public_symbols.is_empty());
}

// Fixture: Python file with public, private, and dunder names.
fn write_python_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/__init__.py"),
        "class Public:\n\
         pass\n\n\
         class _Private:\n\
         pass\n\n\
         def __init__(self):\n\
         pass\n",
    )
    .unwrap();
}

#[test]
fn public_api_card_emits_for_python_non_dunder_names() {
    let repo = tempdir().unwrap();
    write_python_fixture(repo.path());
    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

    let card = compiler.public_api_card("src", Budget::Deep).unwrap();

    let names: Vec<_> = card
        .public_symbols
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    // Public and __init__ should be included, _Private excluded.
    assert!(
        names.contains(&"Public"),
        "Public class must be included; got: {names:?}"
    );
    assert!(
        names.contains(&"__init__"),
        "__init__ must be included; got: {names:?}"
    );
    assert!(
        !names.contains(&"_Private"),
        "_Private must be excluded; got: {names:?}"
    );
}

// Fixture: TypeScript file with export statement and non-exported class.
fn write_typescript_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.ts"),
        "export class Foo {}\n\
         class Bar {}\n",
    )
    .unwrap();
}

#[test]
fn public_api_card_emits_for_typescript_export_decl() {
    let repo = tempdir().unwrap();
    write_typescript_fixture(repo.path());
    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

    let card = compiler.public_api_card("src", Budget::Deep).unwrap();

    let names: Vec<_> = card
        .public_symbols
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    // Foo (exported) must be included.
    assert!(
        names.contains(&"Foo"),
        "Foo must be included; got: {names:?}"
    );
    // Bar: per the design, class-member accessibility_modifier is out of scope
    // for v1, so it defaults to Public. Both are included.
    assert!(
        names.contains(&"Bar"),
        "Bar defaults to Public in v1; got: {names:?}"
    );
}

// Fixture: Go file with capitalized and lowercase functions.
fn write_go_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.go"),
        "package main\n\n\
         func Handle() {}\n\
         func helper() {}\n",
    )
    .unwrap();
}

#[test]
fn public_api_card_emits_for_go_capitalized_ident() {
    let repo = tempdir().unwrap();
    write_go_fixture(repo.path());
    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

    let card = compiler.public_api_card("src", Budget::Deep).unwrap();

    let names: Vec<_> = card
        .public_symbols
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    // Handle (capitalized) should be included, helper (lowercase) excluded.
    assert!(
        names.contains(&"Handle"),
        "Handle must be included; got: {names:?}"
    );
    assert!(
        !names.contains(&"helper"),
        "helper must be excluded; got: {names:?}"
    );
}
