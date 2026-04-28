use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use super::common::{assert_symbol_call, symbol_named};
use crate::{
    config::Config,
    core::ids::NodeId,
    structure::graph::{EdgeKind, GraphStore},
};
use std::fs;
use tempfile::tempdir;

#[test]
fn symbol_calls_retire_when_caller_body_hash_changes() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "fn helper() {}\nfn entry() { helper(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let old_entry = symbol_named(&graph, lib_file.id, "entry");
    let helper = symbol_named(&graph, lib_file.id, "helper");
    assert_symbol_call(&graph, old_entry, helper);

    fs::write(
        repo.path().join("src/lib.rs"),
        "fn helper() {}\nfn entry() { let changed = 1; helper(); let _ = changed; }\n",
    )
    .unwrap();
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let retired_entry = graph.get_symbol(old_entry).unwrap().unwrap();
    assert!(
        retired_entry.retired_at_rev.is_some(),
        "old entry symbol must be retired after body rewrite"
    );
    assert!(
        graph
            .outbound(NodeId::Symbol(old_entry), Some(EdgeKind::Calls))
            .unwrap()
            .is_empty(),
        "old caller edge must be retired with the old caller symbol"
    );

    let new_entry = symbol_named(&graph, lib_file.id, "entry");
    assert_ne!(
        old_entry, new_entry,
        "body hash change must alter symbol ID"
    );
    assert!(
        !graph
            .outbound(NodeId::Symbol(new_entry), Some(EdgeKind::Calls))
            .unwrap()
            .is_empty(),
        "new caller symbol must receive a fresh Calls edge"
    );

    let summary = graph.compact_retired(10_000).unwrap();
    assert!(
        summary.symbols_removed > 0 || summary.edges_removed > 0,
        "compaction must remove retired observations; got: {summary:?}"
    );
    assert!(
        graph.get_symbol(old_entry).unwrap().is_none(),
        "old retired symbol should be physically removed after compaction"
    );
}
