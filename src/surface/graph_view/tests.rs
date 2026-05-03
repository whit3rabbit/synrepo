use time::OffsetDateTime;

use super::{
    build_graph_neighborhood, parse_edge_kind_filter, GraphNeighborhoodRequest, GraphViewDirection,
};
use crate::core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId};
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::structure::graph::{
    ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphStore, MemGraphStore, SymbolKind,
    SymbolNode, Visibility,
};

#[test]
fn target_neighborhood_resolves_short_symbol_and_filters_direction() {
    let graph = sample_graph();
    let model = build_graph_neighborhood(
        &graph,
        GraphNeighborhoodRequest {
            target: Some("lib".to_string()),
            direction: GraphViewDirection::Outbound,
            edge_types: vec![EdgeKind::Calls],
            depth: 1,
            limit: 100,
        },
    )
    .unwrap();

    assert_eq!(
        model.focal_node_id.as_deref(),
        Some("sym_00000000000000000000000000000002")
    );
    assert_eq!(model.direction, "outbound");
    assert_eq!(model.counts.nodes, 2);
    assert_eq!(model.counts.edges, 1);
    assert_eq!(model.edges[0].kind, "calls");
    assert!(model
        .nodes
        .iter()
        .any(|node| node.label == "synrepo::helper"));
}

#[test]
fn target_neighborhood_honors_depth_and_inbound_direction() {
    let graph = sample_graph();
    let model = build_graph_neighborhood(
        &graph,
        GraphNeighborhoodRequest {
            target: Some("src/lib.rs".to_string()),
            direction: GraphViewDirection::Inbound,
            edge_types: Vec::new(),
            depth: 1,
            limit: 100,
        },
    )
    .unwrap();

    assert_eq!(
        model.focal_node_id.as_deref(),
        Some("file_00000000000000000000000000000001")
    );
    assert_eq!(model.counts.nodes, 2);
    assert_eq!(model.counts.edges, 1);
    assert!(model.nodes.iter().any(|node| node.node_type == "concept"));
    assert!(model.edges.iter().all(|edge| edge.kind == "governs"));
}

#[test]
fn top_degree_overview_is_deterministic_and_marks_truncation() {
    let graph = sample_graph();
    let model = build_graph_neighborhood(
        &graph,
        GraphNeighborhoodRequest {
            target: None,
            direction: GraphViewDirection::Both,
            edge_types: Vec::new(),
            depth: 1,
            limit: 1,
        },
    )
    .unwrap();

    assert_eq!(model.focal_node_id, None);
    assert_eq!(model.nodes.len(), 1);
    assert_eq!(model.nodes[0].label, "src/lib.rs");
    assert!(model.truncated);
}

#[test]
fn edge_kind_parser_accepts_camel_and_snake_case() {
    assert_eq!(
        parse_edge_kind_filter("CoChangesWith").unwrap(),
        EdgeKind::CoChangesWith
    );
    assert_eq!(
        parse_edge_kind_filter("co_changes_with").unwrap(),
        EdgeKind::CoChangesWith
    );
}

fn sample_graph() -> MemGraphStore {
    let mut graph = MemGraphStore::new();
    let file_id = FileNodeId(1);
    let symbol_id = SymbolNodeId(2);
    let helper_id = SymbolNodeId(4);
    let concept_id = ConceptNodeId(3);

    graph
        .upsert_file(FileNode {
            id: file_id,
            root_id: "primary".to_string(),
            path: "src/lib.rs".to_string(),
            path_history: Vec::new(),
            content_hash: "file-hash".to_string(),
            size_bytes: 128,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code"),
        })
        .unwrap();
    graph
        .upsert_symbol(symbol(symbol_id, file_id, "synrepo::lib", "lib"))
        .unwrap();
    graph
        .upsert_symbol(symbol(helper_id, file_id, "synrepo::helper", "helper"))
        .unwrap();
    graph
        .upsert_concept(ConceptNode {
            id: concept_id,
            path: "docs/adr/graph.md".to_string(),
            title: "Graph Storage".to_string(),
            aliases: Vec::new(),
            summary: None,
            status: None,
            decision_body: None,
            last_observed_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("parse_prose"),
        })
        .unwrap();
    graph
        .insert_edge(edge(
            10,
            NodeId::File(file_id),
            NodeId::Symbol(symbol_id),
            EdgeKind::Defines,
        ))
        .unwrap();
    graph
        .insert_edge(edge(
            11,
            NodeId::Symbol(symbol_id),
            NodeId::Symbol(helper_id),
            EdgeKind::Calls,
        ))
        .unwrap();
    graph
        .insert_edge(edge(
            12,
            NodeId::Concept(concept_id),
            NodeId::File(file_id),
            EdgeKind::Governs,
        ))
        .unwrap();
    graph.commit().unwrap();
    graph
}

fn symbol(
    id: SymbolNodeId,
    file_id: FileNodeId,
    qualified_name: &str,
    display_name: &str,
) -> SymbolNode {
    SymbolNode {
        id,
        file_id,
        qualified_name: qualified_name.to_string(),
        display_name: display_name.to_string(),
        kind: SymbolKind::Function,
        visibility: Visibility::Public,
        body_byte_range: (0, 16),
        body_hash: format!("{display_name}-hash"),
        signature: Some(format!("fn {display_name}()")),
        doc_comment: None,
        first_seen_rev: None,
        last_modified_rev: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code"),
    }
}

fn edge(id: u128, from: NodeId, to: NodeId, kind: EdgeKind) -> Edge {
    Edge {
        id: EdgeId(id),
        from,
        to,
        kind,
        owner_file_id: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("resolve_edges"),
    }
}

fn sample_provenance(pass: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "rev".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: pass.to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: "src/lib.rs".to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}
