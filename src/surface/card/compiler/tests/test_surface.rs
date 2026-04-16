//! TestSurfaceCard tests.

use super::super::test_support::bootstrap;
use super::super::{Budget, SourceStore};
use super::fixtures::make_compiler;
use crate::surface::card::CardCompiler;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_surface_card_empty_for_no_matching_tests() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    // Create a source file with no associated test files.
    fs::write(repo.path().join("src/lib.rs"), "pub fn foo() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let card = compiler.test_surface_card("src", Budget::Normal).unwrap();

    // No test files exist, so tests should be empty.
    assert_eq!(card.tests.len(), 0);
    assert_eq!(card.test_file_count, 0);
    assert_eq!(card.test_symbol_count, 0);
    assert_eq!(card.source_store, SourceStore::Graph);
}

#[test]
fn test_surface_card_finds_sibling_test_file() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    // Create source file.
    fs::write(repo.path().join("src/lib.rs"), "pub fn foo() {}\n").unwrap();
    // Create sibling test file.
    fs::write(
        repo.path().join("src/lib_test.rs"),
        "#[test] fn test_foo() {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let card = compiler.test_surface_card("src", Budget::Normal).unwrap();

    // Should find the test file.
    assert!(card.test_file_count >= 1);
    assert!(card.test_symbol_count >= 1);
}

#[test]
fn test_surface_card_tiny_budget_returns_counts_only() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn foo() {}\n").unwrap();
    fs::write(
        repo.path().join("src/lib_test.rs"),
        "#[test] fn test_foo() {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let card = compiler.test_surface_card("src", Budget::Tiny).unwrap();

    // Tiny budget should not include individual test entries.
    assert_eq!(card.tests.len(), 0);
    // But counts should still be present.
    assert!(card.test_file_count >= 1 || card.test_symbol_count >= 1);
}
