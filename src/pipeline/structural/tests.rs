use super::*;
use crate::{config::Config, store::sqlite::SqliteGraphStore, structure::graph::GraphStore};
use std::fs;
use tempfile::tempdir;

fn open_graph(repo: &tempfile::TempDir) -> SqliteGraphStore {
    let graph_dir = repo.path().join(".synrepo/graph");
    SqliteGraphStore::open(&graph_dir).unwrap()
}

#[test]
fn structural_compile_populates_file_nodes_and_symbols() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn hello() -> &'static str { \"hi\" }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let summary = run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    assert_eq!(summary.files_discovered, 1);
    assert_eq!(summary.files_parsed, 1);
    assert!(summary.symbols_extracted >= 1);
    assert_eq!(summary.edges_added, summary.symbols_extracted);

    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    assert_eq!(file.language.as_deref(), Some("rust"));
}

#[test]
fn structural_compile_is_idempotent() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn stable() {}\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    let first = run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    let second = run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    assert!(
        first.symbols_extracted >= 1,
        "first run must extract at least one symbol"
    );
    assert_eq!(
        second.files_parsed, 0,
        "second run should skip unchanged files"
    );
    assert_eq!(
        second.symbols_extracted, 0,
        "second run should emit no new symbols"
    );
}

#[test]
fn structural_compile_replaces_stale_symbols_on_content_change() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn old_fn() {}\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    fs::write(repo.path().join("src/lib.rs"), "pub fn new_fn() {}\n").unwrap();
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let (paths, _ids): (Vec<_>, Vec<_>) = graph.all_file_paths().unwrap().into_iter().unzip();
    assert!(paths.contains(&"src/lib.rs".to_string()));

    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let edges = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap();
    assert_eq!(edges.len(), 1);

    let symbol = graph
        .get_symbol(match edges[0].to {
            NodeId::Symbol(id) => id,
            _ => panic!("expected symbol node"),
        })
        .unwrap()
        .unwrap();
    assert_eq!(symbol.display_name, "new_fn");
}

#[test]
fn structural_compile_removes_deleted_files_from_graph() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/to_delete.rs"), "pub fn x() {}\n").unwrap();
    fs::write(repo.path().join("src/keep.rs"), "pub fn y() {}\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    fs::remove_file(repo.path().join("src/to_delete.rs")).unwrap();
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert!(!paths.contains(&"src/to_delete.rs".to_string()));
    assert!(paths.contains(&"src/keep.rs".to_string()));
}

#[test]
fn structural_compile_emits_concept_nodes_from_configured_dirs() {
    let repo = tempdir().unwrap();
    let adr_dir = repo.path().join("docs/adr");
    fs::create_dir_all(&adr_dir).unwrap();
    fs::write(
        adr_dir.join("0001-arch.md"),
        "# Architecture\n\nWhy we built it this way.\n",
    )
    .unwrap();

    let config = Config {
        concept_directories: vec!["docs/adr".to_string()],
        ..Config::default()
    };
    let mut graph = open_graph(&repo);
    let summary = run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    assert_eq!(summary.concept_nodes_emitted, 1);

    let concept_paths: Vec<_> = graph
        .all_concept_paths()
        .unwrap()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert!(concept_paths.contains(&"docs/adr/0001-arch.md".to_string()));
}
