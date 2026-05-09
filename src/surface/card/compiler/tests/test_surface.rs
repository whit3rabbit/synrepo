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
fn test_surface_card_finds_flutter_test_main_harness() {
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("pubspec.yaml"), "name: app\n").unwrap();
    fs::create_dir_all(repo.path().join("lib/src")).unwrap();
    fs::create_dir_all(repo.path().join("test/src")).unwrap();

    fs::write(
        repo.path().join("lib/src/shell.dart"),
        "class Shell { void run() {} }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("test/src/shell_test.dart"),
        "import 'package:app/src/shell.dart';\nvoid main() { Shell().run(); }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let card = compiler
        .test_surface_card("lib/src/shell.dart", Budget::Normal)
        .unwrap();

    assert_eq!(card.test_file_count, 1);
    assert!(
        card.tests.iter().any(|entry| {
            entry.file_path == "test/src/shell_test.dart" && entry.qualified_name == "main"
        }),
        "expected Dart test main harness in {:?}",
        card.tests
    );
}

#[test]
fn test_surface_card_finds_android_junit_method_without_test_name() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("app/src/main/kotlin/com/example")).unwrap();
    fs::create_dir_all(repo.path().join("app/src/test/java/com/example")).unwrap();

    fs::write(
        repo.path().join("app/src/main/kotlin/com/example/Shell.kt"),
        "package com.example\nclass Shell { fun run() {} }\n",
    )
    .unwrap();
    fs::write(
        repo.path()
            .join("app/src/test/java/com/example/ShellTest.java"),
        "package com.example;\npublic class ShellTest { public void usesShell() {} }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let card = compiler
        .test_surface_card("app/src/main/kotlin/com/example/Shell.kt", Budget::Normal)
        .unwrap();

    assert_eq!(card.test_file_count, 1);
    assert!(
        card.tests.iter().any(|entry| {
            entry.file_path == "app/src/test/java/com/example/ShellTest.java"
                && entry.qualified_name == "ShellTest::usesShell"
        }),
        "expected Android JUnit method fallback in {:?}",
        card.tests
    );
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
