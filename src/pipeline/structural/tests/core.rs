use super::super::{ids, run_structural_compile};
use super::support::open_graph;
use crate::{config::Config, core::ids::NodeId, structure::graph::EdgeKind};
use std::fs;
use tempfile::tempdir;

#[test]
fn derive_file_id_is_deterministic() {
    let root = "root-a";
    let path = "src/lib.rs";
    let hash = "abc123def456abc123def456abc123def456abc123def456abc123def456abc1";
    let id1 = ids::derive_file_id(root, path, hash);
    let id2 = ids::derive_file_id(root, path, hash);
    assert_eq!(id1, id2);

    let other = "000000000000000000000000000000000000000000000000000000000000000a";
    assert_ne!(
        ids::derive_file_id(root, path, hash),
        ids::derive_file_id(root, path, other)
    );
    assert_ne!(
        ids::derive_file_id(root, path, hash),
        ids::derive_file_id("root-b", path, hash)
    );
    assert_ne!(
        ids::derive_file_id(root, path, hash),
        ids::derive_file_id(root, "src/other.rs", hash)
    );
}

#[test]
fn identical_files_have_distinct_identities_and_independent_lifecycle() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    let shared_content = "pub fn shared_function() { println!(\"same bytes\"); }\n";
    fs::write(repo.path().join("src/a.rs"), shared_content).unwrap();
    fs::write(repo.path().join("src/b.rs"), shared_content).unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    // 1. Initial compile: both identical files get distinct FileNodeIds
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    let file_a = graph
        .file_by_path("src/a.rs")
        .unwrap()
        .expect("src/a.rs should exist");
    let file_b = graph
        .file_by_path("src/b.rs")
        .unwrap()
        .expect("src/b.rs should exist");
    assert_ne!(
        file_a.id, file_b.id,
        "byte-identical files in the same root must have distinct FileNodeIds"
    );
    assert_eq!(file_a.content_hash, file_b.content_hash);

    let file_b_id = file_b.id;

    // 2. Edit src/a.rs: src/b.rs remains independently addressable with unchanged ID
    fs::write(repo.path().join("src/a.rs"), "pub fn edited_a() {}\n").unwrap();
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let file_b_after_edit = graph.file_by_path("src/b.rs").unwrap().unwrap();
    assert_eq!(file_b_after_edit.id, file_b_id);

    // 3. Delete src/a.rs: src/b.rs remains in the graph intact
    fs::remove_file(repo.path().join("src/a.rs")).unwrap();
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    assert!(graph.file_by_path("src/a.rs").unwrap().is_none());
    let file_b_after_del = graph.file_by_path("src/b.rs").unwrap().unwrap();
    assert_eq!(file_b_after_del.id, file_b_id);
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
fn structural_compile_preserves_file_id_on_rename() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/old.rs"), "pub fn renamed() {}\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    let original_id = graph.file_by_path("src/old.rs").unwrap().unwrap().id;

    fs::rename(
        repo.path().join("src/old.rs"),
        repo.path().join("src/new.rs"),
    )
    .unwrap();

    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    assert!(graph.file_by_path("src/old.rs").unwrap().is_none());

    let new_node = graph
        .file_by_path("src/new.rs")
        .unwrap()
        .expect("new path must be in graph");
    assert_eq!(new_node.id, original_id);
}

#[test]
fn structural_compile_appends_old_path_to_path_history_on_rename() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/original.rs"), "pub fn f() {}\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    fs::rename(
        repo.path().join("src/original.rs"),
        repo.path().join("src/moved.rs"),
    )
    .unwrap();

    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let node = graph
        .file_by_path("src/moved.rs")
        .unwrap()
        .expect("moved path must be in graph");

    assert!(node.path_history.contains(&"src/original.rs".to_string()));
}
