use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    core::{
        ids::{EdgeId, NodeId},
        provenance::Provenance,
    },
    structure::graph::{ConceptNode, Edge, Epistemic, FileNode, SymbolNode},
    surface::card::Budget,
};

#[derive(Serialize)]
pub(super) struct GraphCounts {
    pub(super) nodes: usize,
    pub(super) edges: usize,
    pub(super) files: usize,
    pub(super) symbols: usize,
    pub(super) concepts: usize,
    pub(super) edges_by_kind: BTreeMap<String, usize>,
}

#[derive(Clone, Copy, Default, Serialize)]
pub(super) struct GraphDegree {
    inbound: usize,
    outbound: usize,
    total: usize,
}

#[derive(Serialize)]
struct GraphExportNode {
    id: String,
    #[serde(rename = "type")]
    node_type: &'static str,
    label: String,
    path: Option<String>,
    root_id: Option<String>,
    file_id: Option<String>,
    language: Option<String>,
    symbol_kind: Option<String>,
    visibility: Option<String>,
    degree: GraphDegree,
    epistemic: Epistemic,
    provenance: Provenance,
    metadata: Value,
}

#[derive(Serialize)]
struct GraphExportEdge {
    id: String,
    from: String,
    to: String,
    kind: String,
    label: String,
    owner_file_id: Option<String>,
    drift_score: f32,
    epistemic: Epistemic,
    provenance: Provenance,
}

pub(super) fn file_node(
    file: FileNode,
    budget: Budget,
    degree_by_node: &HashMap<String, GraphDegree>,
) -> impl Serialize {
    let id = NodeId::File(file.id).to_string();
    let metadata = match budget {
        Budget::Deep => json!({
            "content_hash": file.content_hash,
            "size_bytes": file.size_bytes,
            "path_history": file.path_history,
            "inline_decisions": file.inline_decisions,
            "last_observed_rev": file.last_observed_rev,
        }),
        Budget::Tiny | Budget::Normal => json!({
            "content_hash": file.content_hash,
            "size_bytes": file.size_bytes,
        }),
    };

    GraphExportNode {
        id: id.clone(),
        node_type: "file",
        label: file.path.clone(),
        path: Some(file.path),
        root_id: Some(file.root_id),
        file_id: None,
        language: file.language,
        symbol_kind: None,
        visibility: None,
        degree: degree_for(&id, degree_by_node),
        epistemic: file.epistemic,
        provenance: file.provenance,
        metadata,
    }
}

pub(super) fn symbol_node(
    symbol: SymbolNode,
    budget: Budget,
    degree_by_node: &HashMap<String, GraphDegree>,
) -> impl Serialize {
    let id = NodeId::Symbol(symbol.id).to_string();
    let metadata = match budget {
        Budget::Deep => json!({
            "display_name": symbol.display_name,
            "body_byte_range": symbol.body_byte_range,
            "body_hash": symbol.body_hash,
            "signature": symbol.signature,
            "doc_comment": symbol.doc_comment,
            "first_seen_rev": symbol.first_seen_rev,
            "last_modified_rev": symbol.last_modified_rev,
            "last_observed_rev": symbol.last_observed_rev,
        }),
        Budget::Tiny | Budget::Normal => json!({
            "display_name": symbol.display_name,
            "body_hash": symbol.body_hash,
            "signature": symbol.signature,
        }),
    };

    GraphExportNode {
        id: id.clone(),
        node_type: "symbol",
        label: symbol.qualified_name.clone(),
        path: None,
        root_id: None,
        file_id: Some(NodeId::File(symbol.file_id).to_string()),
        language: None,
        symbol_kind: Some(symbol.kind.as_str().to_string()),
        visibility: Some(symbol.visibility.as_str().to_string()),
        degree: degree_for(&id, degree_by_node),
        epistemic: symbol.epistemic,
        provenance: symbol.provenance,
        metadata,
    }
}

pub(super) fn concept_node(
    concept: ConceptNode,
    budget: Budget,
    degree_by_node: &HashMap<String, GraphDegree>,
) -> impl Serialize {
    let id = NodeId::Concept(concept.id).to_string();
    let metadata = match budget {
        Budget::Deep => json!({
            "aliases": concept.aliases,
            "summary": concept.summary,
            "status": concept.status,
            "decision_body": concept.decision_body,
            "last_observed_rev": concept.last_observed_rev,
        }),
        Budget::Tiny | Budget::Normal => json!({
            "summary": concept.summary,
            "status": concept.status,
        }),
    };

    GraphExportNode {
        id: id.clone(),
        node_type: "concept",
        label: concept.title,
        path: Some(concept.path),
        root_id: None,
        file_id: None,
        language: None,
        symbol_kind: None,
        visibility: None,
        degree: degree_for(&id, degree_by_node),
        epistemic: concept.epistemic,
        provenance: concept.provenance,
        metadata,
    }
}

pub(super) fn export_edge(edge: &Edge, drift_scores: &HashMap<EdgeId, f32>) -> impl Serialize {
    GraphExportEdge {
        id: edge.id.to_string(),
        from: edge.from.to_string(),
        to: edge.to.to_string(),
        kind: edge.kind.as_str().to_string(),
        label: edge.kind.as_str().to_string(),
        owner_file_id: edge
            .owner_file_id
            .map(|file_id| NodeId::File(file_id).to_string()),
        drift_score: drift_scores.get(&edge.id).copied().unwrap_or(0.0),
        epistemic: edge.epistemic,
        provenance: edge.provenance.clone(),
    }
}

pub(super) fn summarize_edges(
    edges: &[Edge],
) -> (HashMap<String, GraphDegree>, BTreeMap<String, usize>) {
    let mut degree_by_node = HashMap::<String, GraphDegree>::new();
    let mut edges_by_kind = BTreeMap::<String, usize>::new();

    for edge in edges {
        let from = edge.from.to_string();
        let to = edge.to.to_string();

        let from_degree = degree_by_node.entry(from).or_default();
        from_degree.outbound += 1;
        from_degree.total += 1;

        let to_degree = degree_by_node.entry(to).or_default();
        to_degree.inbound += 1;
        to_degree.total += 1;

        *edges_by_kind
            .entry(edge.kind.as_str().to_string())
            .or_insert(0) += 1;
    }

    (degree_by_node, edges_by_kind)
}

fn degree_for(id: &str, degree_by_node: &HashMap<String, GraphDegree>) -> GraphDegree {
    degree_by_node.get(id).copied().unwrap_or_default()
}
