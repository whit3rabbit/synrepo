use super::*;

#[test]
fn tui_snapshot_can_skip_then_reuse_exact_graph_counts() {
    let _lock = crate::test_support::global_test_lock("status-snapshot-cache");
    let home = tempfile::tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn ready() {}\n").unwrap();
    crate::bootstrap::bootstrap(repo.path(), None, false).unwrap();

    let options = StatusOptions {
        recent: false,
        full: false,
    };
    let exact = build_status_snapshot(repo.path(), options);
    let expected = exact.graph_stats.clone().expect("materialized graph stats");

    let node_only = build_status_snapshot_with_node_stats(repo.path(), options)
        .graph_stats
        .expect("materialized node stats");
    assert_eq!(node_only.file_nodes, expected.file_nodes);
    assert_eq!(node_only.symbol_nodes, expected.symbol_nodes);
    assert_eq!(node_only.concept_nodes, expected.concept_nodes);
    assert_eq!(node_only.total_edges, 0);
    assert!(node_only.edge_counts_by_kind.is_empty());

    let skipped = build_status_snapshot_without_graph_stats(repo.path(), options);
    assert!(skipped.graph_stats.is_none());

    let reused = build_status_snapshot_reusing_expensive_fields(repo.path(), &exact);
    assert_eq!(reused.graph_stats, Some(expected));
}
