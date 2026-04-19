use crate::core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId};
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::store::sqlite::SqliteGraphStore;
use crate::structure::graph::{
    derive_edge_id, ConceptNode, Edge, EdgeKind, Epistemic, FileNode, Graph, GraphReader,
    GraphStore, SymbolKind, SymbolNode, Visibility,
};
use tempfile::tempdir;
use time::OffsetDateTime;

fn sample_provenance(path: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "deadbeef".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "test".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}

fn sample_file(id: u64, path: &str) -> FileNode {
    FileNode {
        id: FileNodeId(id as u128),
        path: path.to_string(),
        path_history: Vec::new(),
        content_hash: format!("file-hash-{id}"),
        size_bytes: 10,
        language: Some("rust".to_string()),
        inline_decisions: vec!["keep it simple".to_string()],
        last_observed_rev: Some(1),
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance(path),
    }
}

fn sample_symbol(id: u64, file_id: FileNodeId, name: &str) -> SymbolNode {
    SymbolNode {
        id: SymbolNodeId(id as u128),
        file_id,
        qualified_name: format!("crate::{name}"),
        display_name: name.to_string(),
        kind: SymbolKind::Function,
        visibility: Visibility::Public,
        body_byte_range: (0, 10),
        body_hash: format!("body-{id}"),
        signature: Some(format!("fn {name}()")),
        doc_comment: Some("docs".to_string()),
        first_seen_rev: Some("abc".to_string()),
        last_modified_rev: Some("def".to_string()),
        last_observed_rev: Some(1),
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("src/lib.rs"),
    }
}

fn sample_concept(id: u64, path: &str) -> ConceptNode {
    ConceptNode {
        id: ConceptNodeId(id as u128),
        path: path.to_string(),
        title: "Concept".to_string(),
        aliases: vec!["alias".to_string()],
        summary: Some("summary".to_string()),
        status: Some("accepted".to_string()),
        decision_body: Some("body".to_string()),
        last_observed_rev: Some(1),
        epistemic: Epistemic::HumanDeclared,
        provenance: sample_provenance(path),
    }
}

#[test]
fn graph_from_store_matches_sqlite_reader_results() {
    let dir = tempdir().unwrap();
    let graph_dir = dir.path().join(".synrepo/graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

    let file = sample_file(1, "src/lib.rs");
    let symbol = sample_symbol(2, file.id, "run");
    let concept = sample_concept(3, "docs/adr/decision.md");
    let edge = Edge {
        id: derive_edge_id(
            NodeId::File(file.id),
            NodeId::Symbol(symbol.id),
            EdgeKind::Defines,
        ),
        from: NodeId::File(file.id),
        to: NodeId::Symbol(symbol.id),
        kind: EdgeKind::Defines,
        epistemic: Epistemic::ParserObserved,
        drift_score: 0.0,
        owner_file_id: Some(file.id),
        last_observed_rev: Some(1),
        retired_at_rev: None,
        provenance: sample_provenance("src/lib.rs"),
    };

    store.begin().unwrap();
    store.upsert_file(file.clone()).unwrap();
    store.upsert_symbol(symbol.clone()).unwrap();
    store.upsert_concept(concept.clone()).unwrap();
    store.insert_edge(edge.clone()).unwrap();
    store.commit().unwrap();

    let graph = Graph::from_store(&store).unwrap();

    assert_eq!(
        graph.get_file(file.id).unwrap().map(|node| node.id),
        store.get_file(file.id).unwrap().map(|node| node.id)
    );
    assert_eq!(
        graph.get_symbol(symbol.id).unwrap().map(|node| node.id),
        store.get_symbol(symbol.id).unwrap().map(|node| node.id)
    );
    assert_eq!(
        graph.get_concept(concept.id).unwrap().map(|node| node.id),
        store.get_concept(concept.id).unwrap().map(|node| node.id)
    );
    assert_eq!(
        graph.file_by_path(&file.path).unwrap().map(|node| node.id),
        store.file_by_path(&file.path).unwrap().map(|node| node.id)
    );
    assert_eq!(
        graph
            .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
            .unwrap()
            .into_iter()
            .map(|edge| edge.id)
            .collect::<Vec<_>>(),
        store
            .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
            .unwrap()
            .into_iter()
            .map(|edge| edge.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        graph.all_file_paths().unwrap(),
        store.all_file_paths().unwrap()
    );
    assert_eq!(
        graph.all_concept_paths().unwrap(),
        store.all_concept_paths().unwrap()
    );
    assert_eq!(
        graph.all_symbol_names().unwrap(),
        store.all_symbol_names().unwrap()
    );
    assert_eq!(
        graph
            .symbols_for_file(file.id)
            .unwrap()
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        store
            .symbols_for_file(file.id)
            .unwrap()
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        graph
            .all_edges()
            .unwrap()
            .into_iter()
            .map(|edge| edge.id)
            .collect::<Vec<_>>(),
        store
            .all_edges()
            .unwrap()
            .into_iter()
            .map(|edge| edge.id)
            .collect::<Vec<_>>()
    );
    assert!(graph.approx_bytes() > 0);
}
