use tempfile::tempdir;

use super::support::{init_empty_graph, seed_files, seed_graph_export_fixture};
use crate::config::Config;
use crate::pipeline::export::{load_manifest, write_exports, ExportFormat, MANIFEST_FILENAME};
use crate::surface::card::Budget;

#[test]
fn export_produces_markdown_files() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Normal,
        true, // --commit: suppress gitignore insertion
    )
    .unwrap();

    let export_dir = repo.path().join("test-export");
    assert!(
        export_dir.join("files.md").exists(),
        "files.md should exist"
    );
    assert!(
        export_dir.join("symbols.md").exists(),
        "symbols.md should exist"
    );
    assert!(
        export_dir.join("decisions.md").exists(),
        "decisions.md should exist"
    );
    assert!(
        export_dir.join(MANIFEST_FILENAME).exists(),
        ".export-manifest.json should exist"
    );
}

#[test]
fn export_produces_json_file() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export-json".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Json,
        Budget::Normal,
        true,
    )
    .unwrap();

    let export_dir = repo.path().join("test-export-json");
    assert!(
        export_dir.join("index.json").exists(),
        "index.json should exist"
    );
}

#[test]
fn manifest_records_correct_format_and_budget() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export-manifest".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Deep,
        true,
    )
    .unwrap();

    let manifest = load_manifest(repo.path(), &config).expect("manifest should load");
    assert_eq!(manifest.format, ExportFormat::Markdown);
    assert_eq!(manifest.budget, "deep");
    assert!(!manifest.generated_at.is_empty());
}

#[test]
fn deep_flag_uses_deep_budget() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export-deep".to_string(),
        ..Config::default()
    };

    let result = write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Deep,
        true,
    )
    .unwrap();

    assert_eq!(result.manifest.budget, "deep");
}

#[test]
fn graph_json_export_round_trips_and_includes_graph_fields() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    let fixture = seed_graph_export_fixture(&synrepo_dir);

    let config = Config {
        export_dir: "graph-json-export".to_string(),
        ..Config::default()
    };

    let result = write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::GraphJson,
        Budget::Normal,
        true,
    )
    .unwrap();

    assert_eq!(result.file_count, 0);
    assert_eq!(result.symbol_count, 0);
    assert_eq!(result.decision_count, 0);
    assert_eq!(result.graph_node_count, 3);
    assert_eq!(result.graph_edge_count, 2);

    let raw = std::fs::read_to_string(repo.path().join("graph-json-export/graph.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["budget"], "normal");
    assert_eq!(parsed["counts"]["nodes"], 3);
    assert_eq!(parsed["counts"]["edges"], 2);

    let nodes = parsed["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|node| {
        node["type"] == "file"
            && node["epistemic"] == "parser_observed"
            && node["metadata"]["content_hash"] == "file-hash"
    }));
    assert!(nodes
        .iter()
        .any(|node| { node["type"] == "concept" && node["epistemic"] == "human_declared" }));

    let edges = parsed["edges"].as_array().unwrap();
    let active = edges
        .iter()
        .find(|edge| edge["id"] == fixture.active_edge_id.to_string())
        .expect("active edge must be exported");
    assert_eq!(active["kind"], "defines");
    assert_eq!(active["drift_score"].as_f64().unwrap(), 0.75);
    assert!(
        !edges
            .iter()
            .any(|edge| edge["id"] == fixture.retired_edge_id.to_string()),
        "retired edges must stay out of graph export"
    );
}

#[test]
fn graph_json_export_handles_empty_graph() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "empty-graph-json".to_string(),
        ..Config::default()
    };

    let result = write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::GraphJson,
        Budget::Normal,
        true,
    )
    .unwrap();
    assert_eq!(result.graph_node_count, 0);
    assert_eq!(result.graph_edge_count, 0);

    let raw = std::fs::read_to_string(repo.path().join("empty-graph-json/graph.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(parsed["nodes"].as_array().unwrap().is_empty());
    assert!(parsed["edges"].as_array().unwrap().is_empty());
}

#[test]
fn graph_html_export_writes_self_contained_view_and_graph_json() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    seed_graph_export_fixture(&synrepo_dir);

    let config = Config {
        export_dir: "graph-html-export".to_string(),
        ..Config::default()
    };

    let result = write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::GraphHtml,
        Budget::Deep,
        true,
    )
    .unwrap();
    assert_eq!(result.graph_node_count, 3);
    assert_eq!(result.graph_edge_count, 2);

    let export_dir = repo.path().join("graph-html-export");
    assert!(export_dir.join("graph.json").exists());
    let html = std::fs::read_to_string(export_dir.join("graph.html")).unwrap();
    assert!(html.contains("<script id=\"graph-data\" type=\"application/json\">"));
    assert!(html.contains("INITIAL_NODE_LIMIT = 250"));
    assert!(html.contains("Expand neighborhood"));
    assert!(html.contains("Path communities"));
    assert!(html.contains("Guided walkthrough"));
    assert!(html.contains("Show drift/change nodes only"));
    assert!(!html.contains("http://"));
    assert!(!html.contains("https://"));
}

#[test]
fn graph_html_export_exposes_card_targets_and_incident_relationships() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    seed_graph_export_fixture(&synrepo_dir);

    let config = Config {
        export_dir: "graph-html-affordances".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::GraphHtml,
        Budget::Deep,
        true,
    )
    .unwrap();

    let html =
        std::fs::read_to_string(repo.path().join("graph-html-affordances/graph.html")).unwrap();
    assert!(html.contains("Card targets"));
    assert!(html.contains("Incident relationships"));
    assert!(html.contains("synrepo_card target="));
    assert!(html.contains("synrepo_minimum_context target="));
    assert!(html.contains("synrepo_context_pack targets="));
    assert!(html.contains("epistemic"));
    assert!(html.contains("drift"));
}

#[test]
fn graph_html_export_caps_large_initial_view_but_keeps_full_data() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    seed_files(&synrepo_dir, 300);

    let config = Config {
        export_dir: "large-graph-html".to_string(),
        ..Config::default()
    };

    let result = write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::GraphHtml,
        Budget::Normal,
        true,
    )
    .unwrap();
    assert_eq!(result.graph_node_count, 300);

    let export_dir = repo.path().join("large-graph-html");
    let raw = std::fs::read_to_string(export_dir.join("graph.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 300);

    let html = std::fs::read_to_string(export_dir.join("graph.html")).unwrap();
    assert!(html.contains("const INITIAL_NODE_LIMIT = 250;"));
}
