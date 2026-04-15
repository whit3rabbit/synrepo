use super::super::SqliteGraphStore;
use super::support::sample_provenance;
use crate::{
    core::ids::{EdgeId, FileNodeId, NodeId, SymbolNodeId},
    structure::graph::{Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode},
};
use tempfile::tempdir;

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
        first_seen_rev: None,
        last_modified_rev: None,
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
            first_seen_rev: None,
            last_modified_rev: None,
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
            first_seen_rev: None,
            last_modified_rev: None,
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
fn delete_edges_by_kind_removes_only_matching_edges() {
    let repo = tempdir().unwrap();
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

    let file_a = FileNode {
        id: FileNodeId(1),
        path: "src/a.rs".to_string(),
        path_history: Vec::new(),
        content_hash: "a".to_string(),
        size_bytes: 10,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/a.rs"),
    };
    let file_b = FileNode {
        id: FileNodeId(2),
        path: "src/b.rs".to_string(),
        path_history: Vec::new(),
        content_hash: "b".to_string(),
        size_bytes: 10,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/b.rs"),
    };
    let file_c = FileNode {
        id: FileNodeId(3),
        path: "src/c.rs".to_string(),
        path_history: Vec::new(),
        content_hash: "c".to_string(),
        size_bytes: 10,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/c.rs"),
    };

    store.upsert_file(file_a.clone()).unwrap();
    store.upsert_file(file_b.clone()).unwrap();
    store.upsert_file(file_c.clone()).unwrap();

    // An Imports edge (should survive).
    store
        .insert_edge(Edge {
            id: EdgeId(10),
            from: NodeId::File(file_a.id),
            to: NodeId::File(file_b.id),
            kind: EdgeKind::Imports,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("stage4", "src/a.rs"),
        })
        .unwrap();

    // Two CoChangesWith edges (should be deleted).
    store
        .insert_edge(Edge {
            id: EdgeId(20),
            from: NodeId::File(file_a.id),
            to: NodeId::File(file_b.id),
            kind: EdgeKind::CoChangesWith,
            epistemic: Epistemic::GitObserved,
            drift_score: 0.0,
            provenance: sample_provenance("stage5_cochange", "src/a.rs"),
        })
        .unwrap();
    store
        .insert_edge(Edge {
            id: EdgeId(21),
            from: NodeId::File(file_a.id),
            to: NodeId::File(file_c.id),
            kind: EdgeKind::CoChangesWith,
            epistemic: Epistemic::GitObserved,
            drift_score: 0.0,
            provenance: sample_provenance("stage5_cochange", "src/a.rs"),
        })
        .unwrap();

    let deleted = store.delete_edges_by_kind(EdgeKind::CoChangesWith).unwrap();
    assert_eq!(deleted, 2);

    // CoChangesWith edges are gone.
    assert!(store
        .outbound(NodeId::File(file_a.id), Some(EdgeKind::CoChangesWith))
        .unwrap()
        .is_empty());

    // Imports edge survives.
    let imports = store
        .outbound(NodeId::File(file_a.id), Some(EdgeKind::Imports))
        .unwrap();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].id, EdgeId(10));
}
