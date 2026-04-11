//! Edge types for the canonical graph.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, str::FromStr};

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
