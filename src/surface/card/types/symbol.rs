use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use crate::overlay::{ConfidenceTier, CrossLinkFreshness, OverlayEdgeKind};
use crate::structure::graph::Epistemic;

use super::super::git::SymbolLastChange;
use super::refs::SymbolRef;
use super::{option_vec_is_empty, ContextAccounting, SourceStore};

/// SymbolCard — answers "what is this function/class, how is it connected?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolCard {
    /// The symbol this card describes.
    pub symbol: SymbolNodeId,
    /// Display name (short form).
    pub name: String,
    /// Fully qualified name within its file.
    pub qualified_name: String,
    /// File and line where defined.
    pub defined_at: String,
    /// File node containing the symbol.
    pub file_id: FileNodeId,
    /// Path relative to the owning root.
    pub path: String,
    /// Discovery root discriminator that owns the containing file.
    pub root_id: String,
    /// True when this symbol belongs to the primary checkout.
    pub is_primary_root: bool,
    /// One-line signature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Doc comment, truncated for `tiny` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// Callers (symbols that call this one). Truncated per budget.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callers: Vec<SymbolRef>,
    /// Callees (symbols this one calls). Truncated per budget.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callees: Vec<SymbolRef>,
    /// Test symbols that exercise this one. Empty for `tiny`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests_touching: Vec<SymbolRef>,
    /// Most recent commit touching this symbol's containing file (V1
    /// granularity: `File`). Absent at `Tiny` budget; revision + author +
    /// timestamp at `Normal`; adds the folded summary at `Deep`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<SymbolLastChange>,
    /// Drift score and flag, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_flag: Option<String>,
    /// Full source body, only populated for `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_body: Option<String>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Context-accounting metadata for this card.
    pub context_accounting: ContextAccounting,
    /// Every field in this card came from the graph; explain commentary
    /// is a separate field below if present.
    pub source_store: SourceStore,
    /// Epistemic origin of the primary fields.
    pub epistemic: Epistemic,
    /// Optional LLM-authored commentary from the overlay, clearly marked.
    /// Only populated if the card was requested at `Deep` budget and
    /// commentary exists in the overlay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay_commentary: Option<OverlayCommentary>,
    /// Flat commentary state label exposed to MCP callers so they can
    /// distinguish `budget_withheld` (Tiny/Normal) from `missing`, `fresh`,
    /// `stale`, `invalid`, or `unsupported` (Deep). Parallel to
    /// `overlay_commentary` so callers can branch on the state without
    /// deserializing the nested object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commentary_state: Option<String>,
    /// Proposed cross-links authored by the explain layer with evidence verification.
    /// Only populated at Deep budget.
    #[serde(default, skip_serializing_if = "option_vec_is_empty")]
    pub proposed_links: Option<Vec<ProposedLink>>,
    /// State of the proposed links (e.g., "budget_withheld", "fresh", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links_state: Option<String>,
}

/// LLM-authored commentary layered on top of a structural card.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayCommentary {
    /// The commentary text.
    pub text: String,
    /// Freshness state of the commentary.
    pub freshness: Freshness,
    /// Source store is always `Overlay` for commentary.
    pub source_store: SourceStore,
}

/// Freshness state of an overlay entry.
///
/// Mirrors the five spec states from `FreshnessState` in `src/overlay/mod.rs`:
/// `Fresh`, `Stale`, `Invalid`, `Missing`, `Unsupported`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// The commentary is current with the source it describes.
    Fresh,
    /// The source has changed since the commentary was produced.
    Stale,
    /// Entry is present but missing one or more required provenance fields.
    Invalid,
    /// No commentary exists for this target yet.
    Missing,
    /// The node kind has no commentary pipeline defined.
    Unsupported,
}

impl From<crate::overlay::FreshnessState> for Freshness {
    fn from(state: crate::overlay::FreshnessState) -> Self {
        match state {
            crate::overlay::FreshnessState::Fresh => Self::Fresh,
            crate::overlay::FreshnessState::Stale => Self::Stale,
            crate::overlay::FreshnessState::Invalid => Self::Invalid,
            crate::overlay::FreshnessState::Missing => Self::Missing,
            crate::overlay::FreshnessState::Unsupported => Self::Unsupported,
        }
    }
}

/// A proposed cross-link surfaced on a structural card.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposedLink {
    /// Node ID of the source.
    pub source: NodeId,
    /// Node ID of the target.
    pub target: NodeId,
    /// Kind of edge proposed.
    pub kind: OverlayEdgeKind,
    /// Confidence tier.
    pub tier: ConfidenceTier,
    /// Freshness of this proposed link compared to the current file content.
    pub freshness: CrossLinkFreshness,
    /// Number of spans cited as evidence.
    pub span_count: usize,
}
