//! Shared fixture helpers for export tests.

use std::fs;

use time::OffsetDateTime;

use crate::core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId};
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::store::sqlite::SqliteGraphStore;
use crate::structure::graph::{
    derive_edge_id, ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind,
    SymbolNode, Visibility,
};

pub(super) struct GraphExportFixture {
    pub(super) active_edge_id: crate::EdgeId,
    pub(super) retired_edge_id: crate::EdgeId,
}

pub(super) fn init_empty_graph(synrepo_dir: &std::path::Path) -> crate::Result<()> {
    let graph_dir = synrepo_dir.join("graph");
    fs::create_dir_all(&graph_dir)?;
    // Open the store to trigger schema creation.
    let _ = crate::store::sqlite::SqliteGraphStore::open(&graph_dir)?;
    Ok(())
}

pub(super) fn seed_files(synrepo_dir: &std::path::Path, count: usize) {
    let graph_dir = synrepo_dir.join("graph");
    fs::create_dir_all(&graph_dir).unwrap();
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    graph.begin().unwrap();
    for i in 0..count {
        let path = format!("src/gen_{i:04}.rs");
        let hash = format!("hash-{i}");
        graph
            .upsert_file(FileNode {
                id: FileNodeId((i as u128) + 1),
                root_id: "primary".to_string(),
                path: path.clone(),
                path_history: Vec::new(),
                content_hash: hash.clone(),
                content_sample_hashes: Vec::new(),
                size_bytes: 128,
                language: Some("rust".to_string()),
                inline_decisions: Vec::new(),
                last_observed_rev: None,
                epistemic: Epistemic::ParserObserved,
                provenance: Provenance {
                    created_at: OffsetDateTime::UNIX_EPOCH,
                    source_revision: "rev".to_string(),
                    created_by: CreatedBy::StructuralPipeline,
                    pass: "parse".to_string(),
                    source_artifacts: vec![SourceRef {
                        file_id: None,
                        path,
                        content_hash: hash,
                    }],
                },
            })
            .unwrap();
    }
    graph.commit().unwrap();
}

pub(super) fn seed_graph_export_fixture(synrepo_dir: &std::path::Path) -> GraphExportFixture {
    let graph_dir = synrepo_dir.join("graph");
    fs::create_dir_all(&graph_dir).unwrap();
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();

    let file_id = FileNodeId(101);
    let symbol_id = SymbolNodeId(202);
    let concept_id = ConceptNodeId(303);
    let file_node = NodeId::File(file_id);
    let symbol_node = NodeId::Symbol(symbol_id);
    let concept_node = NodeId::Concept(concept_id);
    let active_edge_id = derive_edge_id(file_node, symbol_node, EdgeKind::Defines);
    let retired_edge_id = derive_edge_id(symbol_node, concept_node, EdgeKind::References);

    graph.begin().unwrap();
    graph
        .upsert_file(FileNode {
            id: file_id,
            root_id: "primary".to_string(),
            path: "src/lib.rs".to_string(),
            path_history: vec!["src/old_lib.rs".to_string()],
            content_hash: "file-hash".to_string(),
            content_sample_hashes: Vec::new(),
            size_bytes: 64,
            language: Some("rust".to_string()),
            inline_decisions: vec!["keep parser facts canonical".to_string()],
            last_observed_rev: Some(1),
            epistemic: Epistemic::ParserObserved,
            provenance: provenance("parse_code", "src/lib.rs", "file-hash"),
        })
        .unwrap();
    graph
        .upsert_symbol(SymbolNode {
            id: symbol_id,
            file_id,
            qualified_name: "crate::hello".to_string(),
            display_name: "hello".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            body_byte_range: (0, 18),
            body_hash: "symbol-hash".to_string(),
            signature: Some("pub fn hello()".to_string()),
            doc_comment: Some("Say hello.".to_string()),
            first_seen_rev: Some("rev-a".to_string()),
            last_modified_rev: Some("rev-b".to_string()),
            last_observed_rev: Some(1),
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: provenance("parse_code", "src/lib.rs", "symbol-hash"),
        })
        .unwrap();
    graph
        .upsert_concept(ConceptNode {
            id: concept_id,
            path: "docs/decisions/graph.md".to_string(),
            title: "Graph export is deterministic".to_string(),
            aliases: vec!["graph-export".to_string()],
            summary: Some("Exports are local graph views.".to_string()),
            status: Some("accepted".to_string()),
            decision_body: Some("No model call is required.".to_string()),
            last_observed_rev: Some(1),
            epistemic: Epistemic::HumanDeclared,
            provenance: provenance("parse_concepts", "docs/decisions/graph.md", "concept-hash"),
        })
        .unwrap();
    graph
        .insert_edge(Edge {
            id: active_edge_id,
            from: file_node,
            to: symbol_node,
            kind: EdgeKind::Defines,
            epistemic: Epistemic::ParserObserved,
            owner_file_id: Some(file_id),
            last_observed_rev: Some(1),
            retired_at_rev: None,
            provenance: provenance("stage4_defines", "src/lib.rs", "file-hash"),
        })
        .unwrap();
    graph
        .insert_edge(Edge {
            id: derive_edge_id(concept_node, file_node, EdgeKind::Governs),
            from: concept_node,
            to: file_node,
            kind: EdgeKind::Governs,
            epistemic: Epistemic::HumanDeclared,
            owner_file_id: None,
            last_observed_rev: Some(1),
            retired_at_rev: None,
            provenance: provenance("parse_concepts", "docs/decisions/graph.md", "concept-hash"),
        })
        .unwrap();
    graph
        .insert_edge(Edge {
            id: retired_edge_id,
            from: symbol_node,
            to: concept_node,
            kind: EdgeKind::References,
            epistemic: Epistemic::ParserObserved,
            owner_file_id: Some(file_id),
            last_observed_rev: Some(1),
            retired_at_rev: Some(2),
            provenance: provenance("retired_reference", "src/lib.rs", "file-hash"),
        })
        .unwrap();
    graph.commit().unwrap();
    graph
        .write_drift_scores(&[(active_edge_id, 0.75)], "drift-rev")
        .unwrap();

    GraphExportFixture {
        active_edge_id,
        retired_edge_id,
    }
}

fn provenance(pass: &str, path: &str, content_hash: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "rev".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: pass.to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: content_hash.to_string(),
        }],
    }
}
