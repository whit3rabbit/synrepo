use super::*;
use crate::{
    config::Config, pipeline::structural::run_structural_compile, store::sqlite::SqliteGraphStore,
};
use std::fs;
use tempfile::tempdir;

fn make_compiler(graph: SqliteGraphStore, repo: &tempfile::TempDir) -> GraphCardCompiler {
    GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
}

fn bootstrap(repo: &tempfile::TempDir) -> SqliteGraphStore {
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();
    graph
}

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
