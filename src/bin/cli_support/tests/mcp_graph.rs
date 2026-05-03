use std::fs;

use tempfile::tempdir;

use super::support::seed_graph;
use crate::prepare_mcp_state;

#[test]
fn query_via_mcp_resolves_short_symbol_names() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    let state = prepare_mcp_state(repo.path()).expect("MCP state should load");

    let output =
        synrepo::surface::mcp::primitives::handle_query(&state, "outbound lib".to_string());
    let json: serde_json::Value = serde_json::from_str(&output).expect("query should return JSON");

    assert_eq!(json["direction"], "outbound", "unexpected output: {output}");
    assert_eq!(
        json["node_id"],
        ids.symbol_id.to_string(),
        "unexpected output: {output}"
    );
    assert_eq!(json["edges"], serde_json::json!([]));
}

#[test]
fn graph_neighborhood_via_mcp_returns_graph_provenance() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    let state = prepare_mcp_state(repo.path()).expect("MCP state should load");

    let output = synrepo::surface::mcp::graph::handle_graph_neighborhood(
        &state,
        synrepo::surface::mcp::graph::GraphNeighborhoodParams {
            repo_root: None,
            target: Some("src/lib.rs".to_string()),
            direction: Some("outbound".to_string()),
            edge_types: Some(vec!["defines".to_string()]),
            depth: Some(1),
            limit: Some(100),
        },
    );
    let json: serde_json::Value =
        serde_json::from_str(&output).expect("graph_neighborhood should return JSON");

    assert_eq!(json["source_store"], "graph", "unexpected output: {output}");
    assert_eq!(
        json["focal_node_id"],
        ids.file_id.to_string(),
        "unexpected output: {output}"
    );
    assert_eq!(json["counts"]["nodes"], 2);
    assert_eq!(json["counts"]["edges"], 1);
    assert_eq!(json["edges"][0]["kind"], "defines");
    assert_eq!(json["edges"][0]["provenance"]["pass"], "resolve_edges");
    assert_eq!(json["edges"][0]["epistemic"], "parser_observed");
}

#[test]
fn node_via_mcp_accepts_legacy_symbol_prefix_but_returns_canonical_id() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    let state = prepare_mcp_state(repo.path()).expect("MCP state should load");
    let legacy_id = ids.symbol_id.to_string().replacen("sym_", "symbol_", 1);

    let output = synrepo::surface::mcp::primitives::handle_node(&state, legacy_id);
    let json: serde_json::Value = serde_json::from_str(&output).expect("node should return JSON");

    assert_eq!(
        json["node_id"],
        ids.symbol_id.to_string(),
        "unexpected output: {output}"
    );
    assert_eq!(json["node_type"], "symbol");
}

#[test]
fn mcp_source_registers_graph_neighborhood_tool() {
    let source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");
    assert!(
        source.contains("name = \"synrepo_graph_neighborhood\""),
        "MCP registration must include synrepo_graph_neighborhood"
    );
}
