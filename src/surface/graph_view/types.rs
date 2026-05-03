use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Direction used for graph-neighborhood traversal.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphViewDirection {
    /// Traverse inbound and outbound edges.
    #[default]
    Both,
    /// Traverse inbound edges only.
    Inbound,
    /// Traverse outbound edges only.
    Outbound,
}

impl GraphViewDirection {
    /// Parse a user-facing direction value.
    pub fn parse(value: &str) -> crate::Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "both" => Ok(Self::Both),
            "inbound" | "in" => Ok(Self::Inbound),
            "outbound" | "out" => Ok(Self::Outbound),
            other => Err(crate::Error::Other(anyhow::anyhow!(
                "invalid graph view direction `{other}`: expected both, inbound, or outbound"
            ))),
        }
    }

    /// Stable snake_case label for output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }
}

/// Request for a bounded graph-neighborhood model.
#[derive(Clone, Debug)]
pub struct GraphNeighborhoodRequest {
    /// Optional target string: node ID, file path, qualified symbol name, or short symbol name.
    pub target: Option<String>,
    /// Traversal direction.
    pub direction: GraphViewDirection,
    /// Edge kind filters in stable snake_case form.
    pub edge_types: Vec<crate::structure::graph::EdgeKind>,
    /// Requested traversal depth. Clamped to `MAX_DEPTH`.
    pub depth: usize,
    /// Requested node and edge limit. Clamped to `MAX_LIMIT`.
    pub limit: usize,
}

impl Default for GraphNeighborhoodRequest {
    fn default() -> Self {
        Self {
            target: None,
            direction: GraphViewDirection::Both,
            edge_types: Vec::new(),
            depth: 1,
            limit: 100,
        }
    }
}

/// Bounded graph-neighborhood response shared by CLI, TUI, and MCP.
#[derive(Clone, Debug, Serialize)]
pub struct GraphNeighborhood {
    /// Original target string, if provided.
    pub target: Option<String>,
    /// Resolved focal node ID, if a target was provided.
    pub focal_node_id: Option<String>,
    /// Traversal direction label.
    pub direction: &'static str,
    /// Effective clamped traversal depth.
    pub depth: usize,
    /// Effective clamped node/edge limit.
    pub limit: usize,
    /// Edge kind filters applied to traversal.
    pub edge_types: Vec<String>,
    /// Counts for included records.
    pub counts: GraphViewCounts,
    /// True when nodes or edges were omitted because of limits.
    pub truncated: bool,
    /// Included compact graph nodes.
    pub nodes: Vec<GraphViewNode>,
    /// Included compact graph edges.
    pub edges: Vec<GraphViewEdge>,
    /// Source-store label. Graph view data is canonical graph-backed structure.
    pub source_store: &'static str,
}

/// Counts for a graph-neighborhood response.
#[derive(Clone, Debug, Default, Serialize)]
pub struct GraphViewCounts {
    /// Included node count.
    pub nodes: usize,
    /// Included edge count.
    pub edges: usize,
    /// Included file node count.
    pub files: usize,
    /// Included symbol node count.
    pub symbols: usize,
    /// Included concept node count.
    pub concepts: usize,
    /// Included edge counts by edge kind.
    pub edges_by_kind: BTreeMap<String, usize>,
}

/// Compact node in a graph-neighborhood response.
#[derive(Clone, Debug, Serialize)]
pub struct GraphViewNode {
    /// Canonical graph node ID.
    pub id: String,
    /// Node type: file, symbol, or concept.
    #[serde(rename = "type")]
    pub node_type: &'static str,
    /// Human-readable label.
    pub label: String,
    /// Repo-relative path when applicable.
    pub path: Option<String>,
    /// Owning file ID for symbol nodes.
    pub file_id: Option<String>,
    /// Full-graph degree summary after edge-kind filtering.
    pub degree: GraphViewDegree,
}

/// Degree summary for a graph node.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct GraphViewDegree {
    /// Inbound edge count.
    pub inbound: usize,
    /// Outbound edge count.
    pub outbound: usize,
    /// Total incident edge count.
    pub total: usize,
}

/// Compact edge in a graph-neighborhood response.
#[derive(Clone, Debug, Serialize)]
pub struct GraphViewEdge {
    /// Canonical graph edge ID.
    pub id: String,
    /// Source node ID.
    pub from: String,
    /// Target node ID.
    pub to: String,
    /// Edge kind in stable snake_case form.
    pub kind: String,
    /// Drift score for the latest drift revision, or 0 when absent.
    pub drift_score: f32,
    /// Epistemic origin for the edge.
    pub epistemic: crate::structure::graph::Epistemic,
    /// Provenance metadata for the edge.
    pub provenance: crate::core::provenance::Provenance,
}
