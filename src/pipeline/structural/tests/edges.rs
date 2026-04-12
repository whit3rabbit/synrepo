use super::super::run_structural_compile;
use super::support::open_graph;
use crate::{
    config::Config,
    core::ids::NodeId,
    structure::graph::{EdgeKind, GraphStore},
};
use std::fs;
use tempfile::tempdir;

#[test]
fn stage4_emits_calls_edge_for_cross_file_rust_call() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/utils.rs"), "pub fn helper() {}\n").unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "mod utils;\nfn entry() { utils::helper(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let utils_file = graph.file_by_path("src/utils.rs").unwrap().unwrap();
    let helper_sym_id = graph
        .outbound(NodeId::File(utils_file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .find_map(|e| match e.to {
            NodeId::Symbol(id) => Some(id),
            _ => None,
        })
        .expect("helper symbol must exist in graph");

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(lib_file.id), Some(EdgeKind::Calls))
        .unwrap();

    assert!(
        calls.iter().any(|e| e.to == NodeId::Symbol(helper_sym_id)),
        "expected Calls edge from src/lib.rs to helper symbol; got: {calls:?}"
    );
}

#[test]
fn stage4_emits_imports_edge_for_typescript_relative_import() {
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

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let utils_file = graph.file_by_path("src/utils.ts").unwrap().unwrap();

    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();

    assert!(
        imports.iter().any(|e| e.to == NodeId::File(utils_file.id)),
        "expected Imports edge from src/main.ts to src/utils.ts; got: {imports:?}"
    );
}
