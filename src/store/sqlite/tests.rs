use super::{PersistedGraphStats, SqliteGraphStore};
use crate::{
    core::{
        ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId},
        provenance::{Provenance, SourceRef},
    },
    structure::graph::{
        ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode,
    },
};
use std::collections::BTreeMap;
use tempfile::tempdir;
use time::OffsetDateTime;

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
        epistemic: Epistemic::HumanDeclared,
        provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
    };
    let edge = Edge {
        id: EdgeId(0x77),
        from: NodeId::File(file.id),
        to: NodeId::Symbol(symbol.id),
        kind: EdgeKind::Defines,
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
fn deleting_a_file_removes_child_symbols_and_incident_edges() {
    let repo = tempdir().unwrap();
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

    let file = FileNode {
        id: FileNodeId(1),
        path: "src/main.rs".to_string(),
        path_history: Vec::new(),
        content_hash: "main".to_string(),
        size_bytes: 10,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/main.rs"),
    };
    let symbol = SymbolNode {
        id: SymbolNodeId(2),
        file_id: file.id,
        qualified_name: "main".to_string(),
        display_name: "main".to_string(),
        kind: SymbolKind::Function,
        body_byte_range: (0, 10),
        body_hash: "body".to_string(),
        signature: Some("fn main()".to_string()),
        doc_comment: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/main.rs"),
    };
    let edge = Edge {
        id: EdgeId(3),
        from: NodeId::File(file.id),
        to: NodeId::Symbol(symbol.id),
        kind: EdgeKind::Defines,
        epistemic: Epistemic::ParserObserved,
        drift_score: 0.0,
        provenance: sample_provenance("resolve_edges", "src/main.rs"),
    };

    store.upsert_file(file).unwrap();
    store.upsert_symbol(symbol).unwrap();
    store.insert_edge(edge).unwrap();

    store.delete_node(NodeId::File(FileNodeId(1))).unwrap();

    assert!(store.get_file(FileNodeId(1)).unwrap().is_none());
    assert!(store.get_symbol(SymbolNodeId(2)).unwrap().is_none());
    assert!(store
        .outbound(NodeId::File(FileNodeId(1)), None)
        .unwrap()
        .is_empty());
}

#[test]
fn deleting_a_file_removes_edges_for_all_symbols_in_one_pass() {
    let repo = tempdir().unwrap();
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

    let file = FileNode {
        id: FileNodeId(1),
        path: "src/main.rs".to_string(),
        path_history: Vec::new(),
        content_hash: "main".to_string(),
        size_bytes: 10,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/main.rs"),
    };
    let symbols = [
        SymbolNode {
            id: SymbolNodeId(2),
            file_id: file.id,
            qualified_name: "main::one".to_string(),
            display_name: "one".to_string(),
            kind: SymbolKind::Function,
            body_byte_range: (0, 10),
            body_hash: "body-1".to_string(),
            signature: Some("fn one()".to_string()),
            doc_comment: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/main.rs"),
        },
        SymbolNode {
            id: SymbolNodeId(3),
            file_id: file.id,
            qualified_name: "main::two".to_string(),
            display_name: "two".to_string(),
            kind: SymbolKind::Function,
            body_byte_range: (11, 20),
            body_hash: "body-2".to_string(),
            signature: Some("fn two()".to_string()),
            doc_comment: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/main.rs"),
        },
    ];

    store.upsert_file(file.clone()).unwrap();
    for symbol in &symbols {
        store.upsert_symbol(symbol.clone()).unwrap();
    }
    store
        .insert_edge(Edge {
            id: EdgeId(10),
            from: NodeId::File(file.id),
            to: NodeId::Symbol(symbols[0].id),
            kind: EdgeKind::Defines,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/main.rs"),
        })
        .unwrap();
    store
        .insert_edge(Edge {
            id: EdgeId(11),
            from: NodeId::Symbol(symbols[0].id),
            to: NodeId::Symbol(symbols[1].id),
            kind: EdgeKind::References,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/main.rs"),
        })
        .unwrap();
    store
        .insert_edge(Edge {
            id: EdgeId(12),
            from: NodeId::Symbol(symbols[1].id),
            to: NodeId::File(file.id),
            kind: EdgeKind::References,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/main.rs"),
        })
        .unwrap();

    store.delete_node(NodeId::File(file.id)).unwrap();

    assert!(store.get_file(file.id).unwrap().is_none());
    for symbol in &symbols {
        assert!(store.get_symbol(symbol.id).unwrap().is_none());
        assert!(store
            .outbound(NodeId::Symbol(symbol.id), None)
            .unwrap()
            .is_empty());
        assert!(store
            .inbound(NodeId::Symbol(symbol.id), None)
            .unwrap()
            .is_empty());
    }
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

fn sample_provenance(pass: &str, path: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "deadbeef".to_string(),
        created_by: crate::core::provenance::CreatedBy::StructuralPipeline,
        pass: pass.to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}
