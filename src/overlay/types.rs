//! Cross-link types for the overlay store.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::core::ids::NodeId;

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

/// Kinds of edges the explain layer can propose. These land in the
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

impl OverlayEdgeKind {
    /// Stable snake_case identifier. Matches the `#[serde(rename_all = "snake_case")]`
    /// encoding and the stored SQL string used in the overlay store.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::References => "references",
            Self::Governs => "governs",
            Self::DerivedFrom => "derived_from",
            Self::Mentions => "mentions",
        }
    }

    /// Parse a snake_case label back into the variant.
    pub fn from_str_label(label: &str) -> Option<Self> {
        match label {
            "references" => Some(Self::References),
            "governs" => Some(Self::Governs),
            "derived_from" => Some(Self::DerivedFrom),
            "mentions" => Some(Self::Mentions),
            _ => None,
        }
    }
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

/// Surfaced confidence tier for a cross-link candidate.
///
/// The numeric `confidence_score` on a candidate maps into exactly one of
/// these three tiers via [`classify_confidence`]. Card responses expose only
/// the tier; the float is retained on disk for audit and threshold tuning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceTier {
    /// Surfaced in agent-facing card responses at Deep budget.
    High,
    /// Visible through `synrepo links review` and `synrepo findings` but not
    /// in default card responses.
    ReviewQueue,
    /// Withheld from card responses and review queue; visible only in
    /// `synrepo findings` audit output.
    BelowThreshold,
}

impl ConfidenceTier {
    /// Stable snake_case identifier for serialization and logging.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::ReviewQueue => "review_queue",
            Self::BelowThreshold => "below_threshold",
        }
    }
}

/// Observable freshness state of a cross-link candidate.
///
/// Mirrors the five spec states. Both endpoints' hashes must match for
/// `Fresh`; any hash mismatch yields `Stale`; either endpoint missing from the
/// graph yields `SourceDeleted`; a present entry with missing required
/// provenance fields yields `Invalid`; absence of any candidate for a queried
/// pair yields `Missing`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossLinkFreshness {
    /// Both stored endpoint hashes match the current `FileNode.content_hash`.
    Fresh,
    /// One or both stored endpoint hashes no longer match the current graph.
    Stale,
    /// One or both endpoint nodes are no longer in the graph.
    SourceDeleted,
    /// Entry is present but missing one or more required provenance fields.
    Invalid,
    /// No candidate exists for the queried pair.
    Missing,
}

impl CrossLinkFreshness {
    /// Stable snake_case identifier for serialization and logging.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::SourceDeleted => "source_deleted",
            Self::Invalid => "invalid",
            Self::Missing => "missing",
        }
    }
}

/// Provenance record for a cross-link candidate, captured at generation time.
///
/// Every candidate carries one. A missing or empty field yields
/// [`CrossLinkFreshness::Invalid`] when derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossLinkProvenance {
    /// Identifier of the generation pass (e.g. `cross-link-v1`).
    pub pass_id: String,
    /// Identity of the model that produced this candidate (e.g. `claude-sonnet-4-6`).
    pub model_identity: String,
    /// Generation timestamp (RFC 3339 UTC when serialized).
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
}

/// Derived cross-link review state held on disk. Set by the review workflow
/// and the audit/prune paths; surfaced in findings output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossLinkState {
    /// Candidate is active (either awaiting review or surfaced at `High` tier).
    Active,
    /// Candidate is in the mid-promotion window (atomicity bridge).
    PendingPromotion,
    /// Candidate has been promoted into the graph as a `HumanDeclared` edge.
    Promoted,
    /// Candidate has been rejected by a reviewer; excluded from surfacing.
    Rejected,
}

impl CrossLinkState {
    /// Stable snake_case identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::PendingPromotion => "pending_promotion",
            Self::Promoted => "promoted",
            Self::Rejected => "rejected",
        }
    }
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
    /// Content hash of the source endpoint's file at generation time. Used
    /// for freshness derivation against the current graph.
    pub from_content_hash: String,
    /// Content hash of the target endpoint's file at generation time.
    pub to_content_hash: String,
    /// Numeric confidence score in `[0.0, 1.0]`. Persisted for audit and
    /// threshold tuning; never exposed in card responses.
    pub confidence_score: f32,
    /// Surfaced confidence tier derived from the score at generation time.
    pub confidence_tier: ConfidenceTier,
    /// One-sentence LLM rationale, stored for audit but not used by the verifier.
    pub rationale: Option<String>,
    /// Provenance metadata including the pass id, model identity, and timestamp.
    pub provenance: CrossLinkProvenance,
}

impl OverlayLink {
    /// True iff all required `CrossLinkProvenance` fields are non-empty.
    pub fn has_complete_provenance(&self) -> bool {
        !self.provenance.pass_id.is_empty() && !self.provenance.model_identity.is_empty()
    }
}

/// Thresholds for the confidence-tier classifier. Loaded from `Config` by
/// the surface layer; expressed here so the overlay module can derive tiers
/// without importing the config crate.
#[derive(Clone, Copy, Debug)]
pub struct ConfidenceThresholds {
    /// Scores >= this value classify as `High`.
    pub high: f32,
    /// Scores >= this value (and below `high`) classify as `ReviewQueue`.
    /// Scores below this value classify as `BelowThreshold`.
    pub review_queue: f32,
}

impl Default for ConfidenceThresholds {
    fn default() -> Self {
        // Conservative defaults matching design D2 / the cards spec.
        Self {
            high: 0.85,
            review_queue: 0.6,
        }
    }
}

/// Classify a numeric confidence score into a surfaced tier.
pub fn classify_confidence(score: f32, thresholds: ConfidenceThresholds) -> ConfidenceTier {
    if score >= thresholds.high {
        ConfidenceTier::High
    } else if score >= thresholds.review_queue {
        ConfidenceTier::ReviewQueue
    } else {
        ConfidenceTier::BelowThreshold
    }
}

/// Derive the freshness state of a cross-link candidate relative to the
/// current file-content hashes of its endpoints.
///
/// `from_current_hash` / `to_current_hash` are `None` when the endpoint file
/// is no longer present in the graph (i.e. the endpoint was deleted).
pub fn derive_link_freshness(
    link: &OverlayLink,
    from_current_hash: Option<&str>,
    to_current_hash: Option<&str>,
) -> CrossLinkFreshness {
    if !link.has_complete_provenance() {
        return CrossLinkFreshness::Invalid;
    }
    let (Some(from_now), Some(to_now)) = (from_current_hash, to_current_hash) else {
        return CrossLinkFreshness::SourceDeleted;
    };
    if from_now == link.from_content_hash && to_now == link.to_content_hash {
        CrossLinkFreshness::Fresh
    } else {
        CrossLinkFreshness::Stale
    }
}
