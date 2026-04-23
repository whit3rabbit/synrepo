//! Edge types for the canonical graph.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, str::FromStr};

use crate::core::ids::{EdgeId, FileNodeId, NodeId};
use crate::core::provenance::Provenance;

use super::epistemic::Epistemic;

/// Kind of a graph edge.
///
/// Restricted to observed relationships. Edge types the explain layer
/// proposes live on [`crate::overlay::OverlayEdgeKind`] instead.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// `from` imports `to` (module-level import statement).
    Imports,
    /// `from` calls `to` (function call).
    Calls,
    /// `from` inherits from `to` (class inheritance / trait bound).
    Inherits,
    /// `from` defines `to` (file defines symbol, module defines submodule).
    Defines,
    /// `from` references `to` (generic "uses" relationship, parser-observed).
    References,
    /// A markdown file mentions a code identifier (link parser).
    Mentions,
    /// `from` and `to` co-change in git history without an import edge.
    CoChangesWith,
    /// A human-declared ADR or inline marker declares governance.
    /// Only created from frontmatter or inline `# DECISION:` markers,
    /// never inferred.
    Governs,
    /// Provenance: this file was split from another during a refactor.
    SplitFrom,
    /// Provenance: this file was merged from another during a refactor.
    MergedFrom,
}

impl EdgeKind {
    /// Stable snake_case label used for persistence and CLI filtering.
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeKind::Imports => "imports",
            EdgeKind::Calls => "calls",
            EdgeKind::Inherits => "inherits",
            EdgeKind::Defines => "defines",
            EdgeKind::References => "references",
            EdgeKind::Mentions => "mentions",
            EdgeKind::CoChangesWith => "co_changes_with",
            EdgeKind::Governs => "governs",
            EdgeKind::SplitFrom => "split_from",
            EdgeKind::MergedFrom => "merged_from",
        }
    }
}

/// Parse failure for an edge-kind filter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseEdgeKindError {
    value: String,
}

impl fmt::Display for ParseEdgeKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid edge kind: {}", self.value)
    }
}

impl Error for ParseEdgeKindError {}

impl FromStr for EdgeKind {
    type Err = ParseEdgeKindError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "imports" => Ok(Self::Imports),
            "calls" => Ok(Self::Calls),
            "inherits" => Ok(Self::Inherits),
            "defines" => Ok(Self::Defines),
            "references" => Ok(Self::References),
            "mentions" => Ok(Self::Mentions),
            "co_changes_with" => Ok(Self::CoChangesWith),
            "governs" => Ok(Self::Governs),
            "split_from" => Ok(Self::SplitFrom),
            "merged_from" => Ok(Self::MergedFrom),
            _ => Err(ParseEdgeKindError {
                value: value.to_string(),
            }),
        }
    }
}

/// Derive a stable `EdgeId` from `(from_node, to_node, kind)`.
pub fn derive_edge_id(from: NodeId, to: NodeId, kind: EdgeKind) -> EdgeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(from.to_string().as_bytes());
    hasher.update(to.to_string().as_bytes());
    hasher.update(kind.as_str().as_bytes());
    EdgeId(u128::from_le_bytes(
        hasher.finalize().as_bytes()[..16]
            .try_into()
            .expect("blake3 output is 32 bytes"),
    ))
}

/// A graph edge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    /// Stable identifier.
    pub id: EdgeId,
    /// Source node.
    pub from: NodeId,
    /// Target node.
    pub to: NodeId,
    /// Relationship kind.
    pub kind: EdgeKind,
    /// Epistemic origin.
    pub epistemic: Epistemic,
    /// Drift score in [0.0, 1.0]. Always 0.0 at edge creation time.
    /// The canonical drift score is stored in the sidecar `edge_drift` table,
    /// keyed by `(edge_id, revision)`. This field exists for serialization
    /// compatibility but is not kept current in memory.
    pub drift_score: f32,
    /// The file whose parse pass produced this edge. `None` for human-declared
    /// edges and pre-migration rows.
    #[serde(default)]
    pub owner_file_id: Option<FileNodeId>,
    /// Compile revision at which this edge was last observed. `None` for
    /// pre-migration rows.
    #[serde(default)]
    pub last_observed_rev: Option<u64>,
    /// Compile revision at which this edge stopped being emitted. `None`
    /// while the edge is active.
    #[serde(default)]
    pub retired_at_rev: Option<u64>,
    /// Provenance metadata.
    pub provenance: Provenance,
}
