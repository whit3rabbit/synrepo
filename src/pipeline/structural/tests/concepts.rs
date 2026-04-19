use super::super::run_structural_compile;
use super::support::open_graph;
use crate::{
    config::Config,
    core::ids::NodeId,
    structure::graph::{EdgeKind, Epistemic},
};
use std::fs;
use tempfile::tempdir;

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

#[test]
fn stage3_emits_governs_edge_from_adr_frontmatter() {
    let repo = tempdir().unwrap();
    let adr_dir = repo.path().join("docs/adr");
    fs::create_dir_all(&adr_dir).unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/lib.rs"), "pub fn governed() {}\n").unwrap();
    fs::write(
        adr_dir.join("0001-decision.md"),
        "---\ntitle: Storage Decision\nstatus: Accepted\ngoverns: [src/lib.rs]\n---\n\n## Decision\n\nUse SQLite.\n",
    )
    .unwrap();

    let config = Config {
        concept_directories: vec!["docs/adr".to_string()],
        ..Config::default()
    };
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let inbound = graph
        .inbound(NodeId::File(lib_file.id), Some(EdgeKind::Governs))
        .unwrap();

    assert_eq!(inbound.len(), 1, "expected exactly one Governs edge");
    assert!(matches!(inbound[0].from, NodeId::Concept(_)));
    assert_eq!(inbound[0].epistemic, Epistemic::HumanDeclared);
}
