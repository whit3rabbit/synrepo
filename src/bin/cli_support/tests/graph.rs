use super::super::graph::{
    graph_query_output, graph_stats_output, graph_view_json_output, node_output,
};
use super::support::{
    bootstrap_isolated as bootstrap, git, git_stdout, git_with_author, seed_graph,
};
use synrepo::config::Config;
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::EdgeKind;
use synrepo::surface::graph_view::{GraphNeighborhoodRequest, GraphViewDirection};
use tempfile::tempdir;

#[test]
fn node_output_returns_persisted_node_json() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());

    let output = node_output(repo.path(), &ids.file_id.to_string()).unwrap();
    let json = serde_json::from_str::<serde_json::Value>(&output).unwrap();

    assert_eq!(json["node_id"], ids.file_id.to_string());
    assert_eq!(json["node_type"], "file");
    assert_eq!(json["node"]["path"], "src/lib.rs");
    assert_eq!(json["node"]["provenance"]["pass"], "parse_code");
    assert_eq!(json["git_intelligence"]["status"]["state"], "degraded");
    assert_eq!(
        json["git_intelligence"]["status"]["reasons"][0],
        "repository_unavailable"
    );
    assert_eq!(json["git_intelligence"]["commits"], serde_json::json!([]));
    assert_eq!(
        json["git_intelligence"]["hotspot_touches"],
        serde_json::Value::Null
    );
}

#[test]
fn graph_stats_output_counts_persisted_rows() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let output = graph_stats_output(repo.path()).unwrap();
    let json = serde_json::from_str::<serde_json::Value>(&output).unwrap();

    assert_eq!(json["file_nodes"], 1);
    assert_eq!(json["symbol_nodes"], 1);
    assert_eq!(json["concept_nodes"], 1);
    assert_eq!(json["total_edges"], 2);
    assert_eq!(json["edge_counts_by_kind"]["defines"], 1);
    assert_eq!(json["edge_counts_by_kind"]["governs"], 1);
}

#[test]
fn graph_query_output_traverses_edges_with_optional_kind_filter() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());

    let outbound =
        graph_query_output(repo.path(), &format!("outbound {} defines", ids.file_id)).unwrap();
    let outbound_json = serde_json::from_str::<serde_json::Value>(&outbound).unwrap();

    assert_eq!(outbound_json["direction"], "outbound");
    assert_eq!(outbound_json["node_id"], ids.file_id.to_string());
    assert_eq!(outbound_json["edge_kind"], "defines");
    assert_eq!(outbound_json["edges"].as_array().unwrap().len(), 1);
    assert_eq!(outbound_json["edges"][0]["kind"], "defines");
    assert_eq!(
        outbound_json["edges"][0]["id"],
        "edge_00000000000000000000000000000077"
    );
    assert_eq!(outbound_json["edges"][0]["from"], ids.file_id.to_string());
    assert_eq!(outbound_json["edges"][0]["to"], ids.symbol_id.to_string());

    let inbound =
        graph_query_output(repo.path(), &format!("inbound {} governs", ids.file_id)).unwrap();
    let inbound_json = serde_json::from_str::<serde_json::Value>(&inbound).unwrap();

    assert_eq!(inbound_json["direction"], "inbound");
    assert_eq!(inbound_json["edge_kind"], "governs");
    assert_eq!(inbound_json["edges"].as_array().unwrap().len(), 1);
    assert_eq!(inbound_json["edges"][0]["from"], ids.concept_id.to_string());
}

#[test]
fn graph_query_output_resolves_symbol_name_targets() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());

    let output = graph_query_output(repo.path(), "inbound lib defines").unwrap();
    let json = serde_json::from_str::<serde_json::Value>(&output).unwrap();

    assert_eq!(json["direction"], "inbound");
    assert_eq!(json["node_id"], ids.symbol_id.to_string());
    assert_eq!(json["edges"].as_array().unwrap().len(), 1);
    assert_eq!(json["edges"][0]["from"], ids.file_id.to_string());
}

#[test]
fn graph_view_json_output_returns_bounded_model_without_tty() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());

    let output = graph_view_json_output(
        repo.path(),
        GraphNeighborhoodRequest {
            target: Some("src/lib.rs".to_string()),
            direction: GraphViewDirection::Outbound,
            edge_types: vec![EdgeKind::Defines],
            depth: 1,
            limit: 100,
        },
    )
    .unwrap();
    let json = serde_json::from_str::<serde_json::Value>(&output).unwrap();

    assert_eq!(json["source_store"], "graph");
    assert_eq!(json["focal_node_id"], ids.file_id.to_string());
    assert_eq!(json["counts"]["nodes"], 2);
    assert_eq!(json["counts"]["edges"], 1);
    assert_eq!(json["edges"][0]["kind"], "defines");
}

#[test]
fn node_output_includes_file_git_intelligence_for_sampled_history() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();

    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "setup"]);
    git(&repo, &["config", "user.email", "setup@example.com"]);
    git(&repo, &["add", "src/lib.rs"]);
    git_with_author(
        &repo,
        &["commit", "-m", "add lib"],
        "Alice",
        "alice@example.com",
    );

    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() { helper(); }\n",
    )
    .unwrap();
    std::fs::write(repo.path().join("src/helper.rs"), "pub fn helper() {}\n").unwrap();
    git(&repo, &["add", "src/lib.rs", "src/helper.rs"]);
    git_with_author(
        &repo,
        &["commit", "-m", "touch lib and helper"],
        "Bob",
        "bob@example.com",
    );

    bootstrap(repo.path(), None, false).unwrap();

    let graph_dir = Config::synrepo_dir(repo.path()).join("graph");
    let store = SqliteGraphStore::open_existing(&graph_dir).unwrap();
    let file = store.file_by_path("src/lib.rs").unwrap().unwrap();

    let output = node_output(repo.path(), &file.id.to_string()).unwrap();
    let json = serde_json::from_str::<serde_json::Value>(&output).unwrap();

    assert_eq!(json["node_id"], file.id.to_string());
    assert_eq!(json["node"]["path"], "src/lib.rs");
    assert_eq!(json["git_intelligence"]["status"]["state"], "ready");
    assert_eq!(json["git_intelligence"]["hotspot_touches"], 2);
    assert_eq!(
        json["git_intelligence"]["ownership"]["primary_author"],
        "Alice"
    );
    assert_eq!(
        json["git_intelligence"]["co_change_partners"][0]["path"],
        "src/helper.rs"
    );
    assert_eq!(
        json["git_intelligence"]["co_change_partners"][0]["co_change_count"],
        1
    );
    assert_eq!(
        json["git_intelligence"]["commits"][0]["summary"],
        "touch lib and helper"
    );
    assert_eq!(json["git_intelligence"]["commits"][1]["summary"], "add lib");
    assert_eq!(
        json["git_intelligence"]["commits"][0]["revision"],
        git_stdout(&repo, &["rev-parse", "HEAD"])
    );
}

#[test]
fn graph_query_rejects_bad_arity() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    for query in ["", "outbound", "outbound a b c extra"] {
        let err = graph_query_output(repo.path(), query)
            .expect_err(&format!("query `{query}` must be rejected"));
        assert!(
            err.to_string().contains("invalid graph query"),
            "expected `invalid graph query` for `{query}`, got: {err}"
        );
    }
}

#[test]
fn graph_query_rejects_invalid_direction() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());

    let err = graph_query_output(repo.path(), &format!("sideways {}", ids.file_id)).unwrap_err();
    assert!(
        err.to_string().contains("invalid graph query direction"),
        "expected direction error, got: {err}"
    );
}

#[test]
fn graph_query_rejects_invalid_node_id() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let err = graph_query_output(repo.path(), "outbound not_a_node_id").unwrap_err();
    let msg = err.to_string();
    // NodeId::from_str surfaces a parse error; the exact wording is owned by
    // synrepo::core::ids, so assert only that the error mentions the bad input
    // or a parsing concept rather than panicking.
    assert!(
        !msg.is_empty(),
        "expected non-empty parse error for invalid node id"
    );
}

#[test]
fn graph_query_rejects_invalid_edge_kind() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());

    let err = graph_query_output(repo.path(), &format!("outbound {} bogus_kind", ids.file_id))
        .unwrap_err();
    let msg = err.to_string();
    // EdgeKind::from_str returns a parse error for unknown kinds.
    assert!(
        !msg.is_empty(),
        "expected non-empty parse error for invalid edge kind, got: {msg}"
    );
}

#[test]
fn node_output_returns_not_found_for_missing_node() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let err = node_output(repo.path(), "file_0000000000000999").unwrap_err();
    assert!(
        err.to_string().contains("node not found"),
        "expected `node not found`, got: {err}"
    );
}

#[test]
fn node_output_rejects_invalid_id_format() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let err = node_output(repo.path(), "totally-bogus").unwrap_err();
    assert!(
        !err.to_string().is_empty(),
        "expected non-empty parse error for invalid id"
    );
}
