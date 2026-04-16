//! File/symbol card tests: basic budget behavior, imports edges, and
//! signature/doc snapshots. Also covers `entry_point_card` empty-repo safety.

use super::super::test_support::bootstrap;
use super::super::{Budget, GraphCardCompiler, SourceStore};
use super::fixtures::make_compiler;
use crate::{
    core::ids::NodeId,
    store::sqlite::SqliteGraphStore,
    structure::graph::{EdgeKind, GraphStore},
    surface::card::CardCompiler,
};
use insta::assert_snapshot;
use std::fs;
use tempfile::tempdir;

#[test]
fn file_card_returns_defined_symbols() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn foo() {}\npub fn bar() {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
    let compiler = make_compiler(graph, &repo);

    let card = compiler.file_card(file_id, Budget::Tiny).unwrap();
    assert_eq!(card.path, "src/lib.rs");
    assert_eq!(card.symbols.len(), 2);
    let names: Vec<&str> = card
        .symbols
        .iter()
        .map(|s| s.qualified_name.as_str())
        .collect();
    assert!(names.contains(&"foo"), "expected foo in {names:?}");
    assert!(names.contains(&"bar"), "expected bar in {names:?}");
}

#[test]
fn resolve_target_finds_by_path_and_by_name() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn my_func() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let by_path = compiler.resolve_target("src/lib.rs").unwrap();
    assert!(matches!(by_path, Some(NodeId::File(_))));

    let by_name = compiler.resolve_target("my_func").unwrap();
    assert!(matches!(by_name, Some(NodeId::Symbol(_))));

    assert!(compiler.resolve_target("nonexistent").unwrap().is_none());
}

#[test]
fn symbol_card_tiny_has_no_source_body() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Docs.\npub fn documented() -> u32 { 42 }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let sym_edge = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let sym_id = match sym_edge.to {
        NodeId::Symbol(id) => id,
        _ => panic!("expected symbol"),
    };
    let compiler = make_compiler(graph, &repo);

    let tiny = compiler.symbol_card(sym_id, Budget::Tiny).unwrap();
    assert_eq!(tiny.name, "documented");
    assert!(
        tiny.source_body.is_none(),
        "tiny budget must not include source body"
    );
    assert!(tiny.approx_tokens > 0);
    assert_eq!(tiny.source_store, SourceStore::Graph);

    let graph2 = {
        let graph_dir = repo.path().join(".synrepo/graph");
        SqliteGraphStore::open(&graph_dir).unwrap()
    };
    let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
    let normal = compiler2.symbol_card(sym_id, Budget::Normal).unwrap();
    assert!(
        normal.source_body.is_none(),
        "normal budget must not include source body"
    );

    let graph3 = {
        let graph_dir = repo.path().join(".synrepo/graph");
        SqliteGraphStore::open(&graph_dir).unwrap()
    };
    let compiler3 = GraphCardCompiler::new(Box::new(graph3), Some(repo.path()));
    let deep = compiler3.symbol_card(sym_id, Budget::Deep).unwrap();
    assert!(
        deep.source_body.is_some(),
        "deep budget must include source body"
    );
    let body = deep.source_body.unwrap();
    assert!(
        body.contains("documented"),
        "source body must contain function text"
    );
}

#[test]
fn file_card_includes_imports_edges() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/utils.ts"),
        "export function helper() {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "import { helper } from './utils';\nhelper();\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let main_id = graph.file_by_path("src/main.ts").unwrap().unwrap().id;
    let utils_id = graph.file_by_path("src/utils.ts").unwrap().unwrap().id;
    let compiler = make_compiler(graph, &repo);

    let card = compiler.file_card(main_id, Budget::Normal).unwrap();
    assert!(
        card.imports.iter().any(|r| r.id == utils_id),
        "main.ts card must list utils.ts as an import"
    );

    let graph2 = {
        let graph_dir = repo.path().join(".synrepo/graph");
        SqliteGraphStore::open(&graph_dir).unwrap()
    };
    let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
    let utils_card = compiler2.file_card(utils_id, Budget::Normal).unwrap();
    assert!(
        utils_card.imported_by.iter().any(|r| r.id == main_id),
        "utils.ts card must list main.ts in imported_by"
    );
}

#[test]
fn symbol_card_snapshots_with_signature_and_doc_comment() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Add two integers together.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let sym_edge = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let sym_id = match sym_edge.to {
        NodeId::Symbol(id) => id,
        _ => panic!("expected symbol"),
    };
    let compiler = make_compiler(graph, &repo);

    // Snapshot all three budget tiers so regressions are visible.
    let tiny =
        serde_json::to_string_pretty(&compiler.symbol_card(sym_id, Budget::Tiny).unwrap()).unwrap();
    assert_snapshot!("symbol_card_tiny", tiny);

    let normal =
        serde_json::to_string_pretty(&compiler.symbol_card(sym_id, Budget::Normal).unwrap())
            .unwrap();
    assert_snapshot!("symbol_card_normal", normal);

    let deep =
        serde_json::to_string_pretty(&compiler.symbol_card(sym_id, Budget::Deep).unwrap()).unwrap();
    assert_snapshot!("symbol_card_deep", deep);
}

// 7.5: entry_point_card returns empty list (no panic) when no files are indexed
#[test]
fn entry_point_card_empty_repo_returns_no_panic() {
    let repo = tempdir().unwrap();
    // Bootstrap produces an empty graph (no source files to index).
    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let card = compiler
        .entry_point_card(None, Budget::Tiny)
        .expect("entry_point_card must not error on empty graph");
    assert!(
        card.entry_points.is_empty(),
        "empty graph must produce empty entry_points list"
    );
    assert_eq!(card.source_store, SourceStore::Graph);
}
