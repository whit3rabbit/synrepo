//! The overlay store: LLM-authored content, physically separate from the graph.
//!
//! This module is intentionally thin in phase 0/1. The overlay exists
//! architecturally but is empty until phase 4 adds commentary and phase 5
//! adds cross-link proposals.
//!
//! The key invariant: the synthesis pipeline never reads its own previous
//! output as retrieval input. This is enforced both physically (overlay
//! lives in a separate sqlite database from the graph) and at the retrieval
//! layer (synthesis queries filter on `source_store = "graph"`).

use serde::{Deserialize, Serialize};

use crate::core::ids::NodeId;
use crate::core::provenance::Provenance;

/// Epistemic origin of an overlay entry. These variants cannot appear in
/// the canonical graph — the type system enforces the boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayEpistemic {
    /// LLM-proposed with strong cited evidence and short graph distance.
    MachineAuthoredHighConf,
    /// LLM-proposed with weak evidence.
    MachineAuthoredLowConf,
}

/// Kinds of edges the synthesis layer can propose. These land in the
/// overlay's `cross_links` table, never in the canonical graph's `edges` table.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayEdgeKind {
    /// A prose document references a code symbol.
    References,
    /// A prose document describes governance of a code artifact,
    /// proposed (not human-declared — the `Governs` edge in the graph
    /// is the human-declared variant).
    Governs,
    /// One concept is derived from another.
    DerivedFrom,
    /// A prose document mentions a code identifier.
    Mentions,
}

/// A cited span — verbatim text the LLM extracted from a source artifact,
/// verified by fuzzy LCS match against the actual source after normalization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CitedSpan {
    /// The file or concept the span was cited from.
    pub artifact: NodeId,
    /// Normalized form of the span text (whitespace collapsed, Unicode
    /// quotes normalized, etc.). This is what the verifier compared against.
    pub normalized_text: String,
    /// Byte offset in the source artifact where the verifier found a match
    /// after fuzzy snapping. This is the authoritative location, not the
    /// offset the LLM claimed.
    pub verified_at_offset: u32,
    /// LCS ratio from the fuzzy match (in `[0.0, 1.0]`; >= 0.9 for default
    /// threshold, tunable per provider).
    pub lcs_ratio: f32,
}

/// A proposed cross-link in the overlay.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayLink {
    /// Source node.
    pub from: NodeId,
    /// Target node.
    pub to: NodeId,
    /// Relationship kind.
    pub kind: OverlayEdgeKind,
    /// Epistemic origin.
    pub epistemic: OverlayEpistemic,
    /// Cited spans from the source artifact.
    pub source_spans: Vec<CitedSpan>,
    /// Cited spans from the target artifact.
    pub target_spans: Vec<CitedSpan>,
    /// Confidence score derived from span count, span length, LCS ratios,
    /// and graph distance.
    pub confidence: f32,
    /// One-sentence LLM rationale, stored for audit but not used by the verifier.
    pub rationale: Option<String>,
    /// Provenance metadata including the prompt version and model ID.
    pub provenance: Provenance,
}

/// Trait for the overlay store. Phase 0/1 does not implement this; it
/// exists so that graph callers can be written with the right architectural
/// separation in place from the start.
pub trait OverlayStore: Send + Sync {
    /// Insert a proposed cross-link.
    fn insert_link(&mut self, link: OverlayLink) -> crate::Result<()>;

    /// Return all overlay links involving a given node (as source or target).
    fn links_for(&self, node: NodeId) -> crate::Result<Vec<OverlayLink>>;

    /// Commit any pending writes.
    fn commit(&mut self) -> crate::Result<()>;
}
