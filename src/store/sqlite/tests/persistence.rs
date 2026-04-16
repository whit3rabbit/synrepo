use super::super::{PersistedGraphStats, SqliteGraphStore};
use super::support::sample_provenance;
use crate::{
    core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId},
    structure::graph::{
        ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode,
    },
};
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn graph_store_round_trips_nodes_edges_and_provenance() {
    let repo = tempdir().unwrap();
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

    let file = FileNode {
        id: FileNodeId(0x42),
        path: "src/lib.rs".to_string(),
        path_history: vec!["src/old_lib.rs".to_string()],
        content_hash: "abc123".to_string(),
        size_bytes: 128,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        last_observed_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/lib.rs"),
    };
    let symbol = SymbolNode {
        id: SymbolNodeId(0x24),
        file_id: file.id,
        qualified_name: "synrepo::lib".to_string(),
        display_name: "lib".to_string(),
        kind: SymbolKind::Module,
        body_byte_range: (0, 64),
        body_hash: "def456".to_string(),
        signature: Some("pub mod lib".to_string()),
        doc_comment: None,
        first_seen_rev: None,
        last_modified_rev: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/lib.rs"),
    };
    let concept = ConceptNode {
        id: ConceptNodeId(0x99),
        path: "docs/adr/0001-graph.md".to_string(),
        title: "Graph Storage".to_string(),
        aliases: vec!["canonical-graph".to_string()],
        summary: Some("Why the graph stays observed-only.".to_string()),
        status: None,
        decision_body: None,
        last_observed_rev: None,
        epistemic: Epistemic::HumanDeclared,
        provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
    };
    let edge = Edge {
        id: EdgeId(0x77),
        from: NodeId::File(file.id),
        to: NodeId::Symbol(symbol.id),
        kind: EdgeKind::Defines,
        owner_file_id: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        drift_score: 0.0,
        provenance: sample_provenance("resolve_edges", "src/lib.rs"),
    };

    store.begin().unwrap();
    store.upsert_file(file.clone()).unwrap();
    store.upsert_symbol(symbol.clone()).unwrap();
    store.upsert_concept(concept.clone()).unwrap();
    store.insert_edge(edge.clone()).unwrap();
    store.commit().unwrap();

    let loaded_file = store.get_file(file.id).unwrap().unwrap();
    let loaded_symbol = store.get_symbol(symbol.id).unwrap().unwrap();
    let loaded_concept = store.get_concept(concept.id).unwrap().unwrap();
    let outbound = store.outbound(NodeId::File(file.id), None).unwrap();

    assert_eq!(loaded_file.path, file.path);
    assert_eq!(loaded_file.path_history, file.path_history);
    assert_eq!(loaded_file.provenance.pass, "parse_code");
    assert_eq!(loaded_symbol.qualified_name, symbol.qualified_name);
    assert_eq!(loaded_symbol.body_hash, symbol.body_hash);
    assert_eq!(loaded_concept.title, concept.title);
    assert_eq!(loaded_concept.epistemic, Epistemic::HumanDeclared);
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0].kind, EdgeKind::Defines);
    assert_eq!(outbound[0].to, NodeId::Symbol(symbol.id));
    assert_eq!(
        store.file_by_path("src/lib.rs").unwrap().unwrap().id,
        FileNodeId(0x42)
    );
    assert!(SqliteGraphStore::db_path(&graph_dir).exists());
}

#[test]
fn open_existing_requires_materialized_graph_store() {
    let repo = tempdir().unwrap();
    let error = SqliteGraphStore::open_existing(&repo.path().join(".synrepo/graph"))
        .err()
        .unwrap()
        .to_string();

    assert!(error.contains("graph store is not materialized"));
}

#[test]
fn persisted_stats_count_nodes_and_edges_by_kind() {
    let repo = tempdir().unwrap();
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

    let file = FileNode {
        id: FileNodeId(10),
        path: "src/lib.rs".to_string(),
        path_history: Vec::new(),
        content_hash: "a".to_string(),
        size_bytes: 1,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        last_observed_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/lib.rs"),
    };
    let symbol = SymbolNode {
        id: SymbolNodeId(11),
        file_id: file.id,
        qualified_name: "crate::lib".to_string(),
        display_name: "lib".to_string(),
        kind: SymbolKind::Module,
        body_byte_range: (0, 1),
        body_hash: "b".to_string(),
        signature: None,
        doc_comment: None,
        first_seen_rev: None,
        last_modified_rev: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/lib.rs"),
    };
    let concept = ConceptNode {
        id: ConceptNodeId(12),
        path: "docs/adr/0001.md".to_string(),
        title: "Decision".to_string(),
        aliases: Vec::new(),
        summary: None,
        status: None,
        decision_body: None,
        last_observed_rev: None,
        epistemic: Epistemic::HumanDeclared,
        provenance: sample_provenance("parse_prose", "docs/adr/0001.md"),
    };

    store.upsert_file(file.clone()).unwrap();
    store.upsert_symbol(symbol.clone()).unwrap();
    store.upsert_concept(concept).unwrap();
    store
        .insert_edge(Edge {
            id: EdgeId(13),
            from: NodeId::File(file.id),
            to: NodeId::Symbol(symbol.id),
            kind: EdgeKind::Defines,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/lib.rs"),
        })
        .unwrap();
    store
        .insert_edge(Edge {
            id: EdgeId(14),
            from: NodeId::Symbol(symbol.id),
            to: NodeId::File(file.id),
            kind: EdgeKind::References,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/lib.rs"),
        })
        .unwrap();

    let stats = store.persisted_stats().unwrap();

    assert_eq!(
        stats,
        PersistedGraphStats {
            file_nodes: 1,
            symbol_nodes: 1,
            concept_nodes: 1,
            total_edges: 2,
            edge_counts_by_kind: BTreeMap::from([
                ("defines".to_string(), 1),
                ("references".to_string(), 1),
            ]),
        }
    );
}
