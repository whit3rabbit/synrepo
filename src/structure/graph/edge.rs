//! Edge types for the canonical graph.

use serde::{Deserialize, Serialize};

use crate::core::ids::{EdgeId, NodeId};
use crate::core::provenance::Provenance;

use super::epistemic::Epistemic;

/// Kind of a graph edge.
///
/// Restricted to observed relationships. Edge types the synthesis layer
/// proposes live on [`crate::overlay::OverlayEdgeKind`] instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    /// Drift score in [0.0, 1.0]; 0 means fresh, 1 means maximally drifted.
    /// Updated by the structural pipeline on every commit.
    pub drift_score: f32,
    /// Provenance metadata.
    pub provenance: Provenance,
}
