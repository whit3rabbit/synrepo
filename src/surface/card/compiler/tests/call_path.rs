//! CallPathCard tests.

use super::super::test_support::bootstrap;
use super::super::{Budget, SourceStore};
use super::fixtures::make_compiler;
use crate::{core::ids::SymbolNodeId, surface::card::CardCompiler};
use std::fs;
use tempfile::tempdir;

#[test]
fn call_path_card_empty_for_unknown_symbol() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn foo() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    // Use a non-existent symbol ID.
    let fake_id = SymbolNodeId(999999);
    let card = compiler.call_path_card(fake_id, Budget::Normal).unwrap();

    assert_eq!(card.paths.len(), 0);
    assert_eq!(card.paths_omitted, 0);
    assert_eq!(card.source_store, SourceStore::Graph);
}

#[test]
fn call_path_card_empty_for_symbol_with_no_callers() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    // A function with no callers - should return empty paths.
    fs::write(repo.path().join("src/lib.rs"), "pub fn isolated() {}\n").unwrap();

    let graph = bootstrap(&repo);

    // Get the symbol ID before creating compiler (graph will be moved).
    let symbols = graph.all_symbol_names().unwrap();
    let isolated_id = symbols
        .iter()
        .find(|(_, _, qname)| qname == "isolated")
        .map(|(id, _, _)| *id)
        .unwrap();

    let compiler = make_compiler(graph, &repo);

    let card = compiler
        .call_path_card(isolated_id, Budget::Normal)
        .unwrap();

    // Should return empty paths since no callers exist.
    assert_eq!(card.paths.len(), 0);
    assert_eq!(card.paths_omitted, 0);
}
