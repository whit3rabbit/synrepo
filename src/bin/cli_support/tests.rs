use super::{
    commands::search,
    graph::{graph_query_output, graph_stats_output, node_output},
};
use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::core::ids::{ConceptNodeId, EdgeId, FileNodeId, SymbolNodeId};
use synrepo::core::provenance::{CreatedBy, Provenance, SourceRef};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{
    ConceptNode, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode,
};
use synrepo::NodeId;
use tempfile::tempdir;
use time::OffsetDateTime;

#[test]
fn search_requires_rebuild_when_index_sensitive_config_changes() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "search token\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let updated = Config {
        roots: vec!["src".to_string()],
        ..Config::load(repo.path()).unwrap()
    };
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        toml::to_string_pretty(&updated).unwrap(),
    )
    .unwrap();

    let error = search(repo.path(), "search token").unwrap_err().to_string();

    assert!(error.contains("Storage compatibility"));
    assert!(error.contains("requires rebuild"));
}

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
    assert_eq!(outbound_json["edges"][0]["id"], "edge_0000000000000077");
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

struct SeededGraphIds {
    file_id: FileNodeId,
    symbol_id: SymbolNodeId,
    concept_id: ConceptNodeId,
}

fn seed_graph(repo_root: &std::path::Path) -> SeededGraphIds {
    bootstrap(repo_root, None).unwrap();

    let graph_dir = Config::synrepo_dir(repo_root).join("graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();
    let file_id = FileNodeId(0x42);
    let symbol_id = SymbolNodeId(0x24);
    let concept_id = ConceptNodeId(0x99);

    store.begin().unwrap();
    store
        .upsert_file(FileNode {
            id: file_id,
            path: "src/lib.rs".to_string(),
            path_history: vec!["src/old_lib.rs".to_string()],
            content_hash: "abc123".to_string(),
            size_bytes: 128,
            language: Some("rust".to_string()),
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        })
        .unwrap();
    store
        .upsert_symbol(SymbolNode {
            id: symbol_id,
            file_id,
            qualified_name: "synrepo::lib".to_string(),
            display_name: "lib".to_string(),
            kind: SymbolKind::Module,
            body_byte_range: (0, 64),
            body_hash: "def456".to_string(),
            signature: Some("pub mod lib".to_string()),
            doc_comment: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        })
        .unwrap();
    store
        .upsert_concept(ConceptNode {
            id: concept_id,
            path: "docs/adr/0001-graph.md".to_string(),
            title: "Graph Storage".to_string(),
            aliases: vec!["canonical-graph".to_string()],
            summary: Some("Why the graph stays observed-only.".to_string()),
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
        })
        .unwrap();
    store
        .insert_edge(synrepo::structure::graph::Edge {
            id: EdgeId(0x77),
            from: NodeId::File(file_id),
            to: NodeId::Symbol(symbol_id),
            kind: EdgeKind::Defines,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/lib.rs"),
        })
        .unwrap();
    store
        .insert_edge(synrepo::structure::graph::Edge {
            id: EdgeId(0x78),
            from: NodeId::Concept(concept_id),
            to: NodeId::File(file_id),
            kind: EdgeKind::Governs,
            epistemic: Epistemic::HumanDeclared,
            drift_score: 0.2,
            provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
        })
        .unwrap();
    store.commit().unwrap();

    SeededGraphIds {
        file_id,
        symbol_id,
        concept_id,
    }
}

fn sample_provenance(pass: &str, path: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "deadbeef".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: pass.to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}

#[test]
fn reconcile_completes_on_initialized_repo() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() {}\n",
    )
    .unwrap();
    // bootstrap sets up .synrepo/ and runs the first compile.
    bootstrap(repo.path(), None).unwrap();

    // reconcile must succeed and persist state.
    super::commands::reconcile(repo.path()).unwrap();

    let synrepo_dir = synrepo::config::Config::synrepo_dir(repo.path());
    let state = synrepo::pipeline::watch::load_reconcile_state(&synrepo_dir)
        .expect("reconcile state must be written after reconcile");
    assert_eq!(state.last_outcome, "completed");
    assert!(
        state.files_discovered.unwrap_or(0) >= 1,
        "reconcile must discover at least src/lib.rs"
    );
}
