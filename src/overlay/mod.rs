//! The overlay store: LLM-authored content, physically separate from the graph.
//!
//! Defines the [`OverlayStore`] trait and its payload types: commentary
//! entries (with provenance, freshness derivation, and staleness repair), plus
//! cross-link proposals (stubbed; activated in phase 5+).
//!
//! The key invariant: the synthesis pipeline never reads its own previous
//! output as retrieval input. This is enforced both physically (overlay
//! lives in a separate sqlite database from the graph) and at the retrieval
//! layer (synthesis queries filter on `source_store = "graph"`).

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

/// Provenance record for a single commentary entry.
///
/// Every commentary entry carries one of these; all fields are required and
/// validated on insert. A missing or empty field yields `FreshnessState::Invalid`
/// when derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentaryProvenance {
    /// Content hash (typically the annotated node's file's `content_hash`) at
    /// the time this commentary was generated. Used for freshness derivation.
    pub source_content_hash: String,
    /// Identifier of the generation pass that produced this entry (e.g. a
    /// human-readable pass name or a deterministic pass ID).
    pub pass_id: String,
    /// Identity of the model that produced this entry (e.g. `claude-sonnet-4-6`).
    pub model_identity: String,
    /// Generation timestamp (RFC 3339 UTC when serialized).
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
}

/// A single commentary entry persisted in the overlay store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommentaryEntry {
    /// The node this commentary annotates.
    pub node_id: NodeId,
    /// The commentary body text.
    pub text: String,
    /// Provenance record for this entry.
    pub provenance: CommentaryProvenance,
}

/// Observable freshness state of a commentary entry.
///
/// Mirrors the five spec states: a match against the current source yields
/// `Fresh`; a mismatch yields `Stale`; a present entry with missing
/// provenance yields `Invalid`; absence of any entry yields `Missing`; a
/// node kind with no commentary pipeline yields `Unsupported`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    /// Stored `source_content_hash` matches the current `FileNode.content_hash`.
    Fresh,
    /// Stored `source_content_hash` does not match the current source.
    Stale,
    /// Entry is present but missing one or more required provenance fields.
    Invalid,
    /// No entry exists for the queried node.
    Missing,
    /// The node kind has no commentary pipeline defined.
    Unsupported,
}

impl FreshnessState {
    /// Stable snake_case identifier for serialization and logging.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Invalid => "invalid",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Trait for the overlay store. Ships commentary and cross-link persistence
/// via `SqliteOverlayStore`. The storage layer enforces physical isolation
/// from the canonical graph store.
pub trait OverlayStore: Send + Sync {
    /// Insert a proposed cross-link candidate.
    ///
    /// Rejects candidates with missing provenance, empty source or target
    /// spans, or hashes that would leave freshness underivable.
    fn insert_link(&mut self, link: OverlayLink) -> crate::Result<()>;

    /// Return all overlay links involving a given node (as source or target).
    fn links_for(&self, node: NodeId) -> crate::Result<Vec<OverlayLink>>;

    /// Commit any pending writes (no-op for auto-commit SQLite connections).
    fn commit(&mut self) -> crate::Result<()>;

    /// Upsert a commentary entry, keyed on `node_id`. Rejects entries whose
    /// provenance is missing one or more required fields.
    fn insert_commentary(&mut self, entry: CommentaryEntry) -> crate::Result<()>;

    /// Return the commentary entry for a node, or `None` if absent.
    fn commentary_for(&self, node: NodeId) -> crate::Result<Option<CommentaryEntry>>;

    /// Delete all commentary entries and cross-link candidates whose endpoint
    /// node IDs are not present in `live_nodes`. Pruned cross-links leave an
    /// immutable audit row. Returns the total number of rows deleted.
    fn prune_orphans(&mut self, live_nodes: &[NodeId]) -> crate::Result<usize>;

    /// Open a read snapshot on this store. Reads through this handle
    /// observe a single committed epoch until `end_read_snapshot` is
    /// called. Same contract as [`crate::structure::graph::GraphStore::begin_read_snapshot`]:
    /// must be paired, must not nest, must not interleave with writes on
    /// the same handle. Default no-op.
    fn begin_read_snapshot(&self) -> crate::Result<()> {
        Ok(())
    }

    /// Close a read snapshot opened by `begin_read_snapshot`. Tolerates
    /// being called when no snapshot is active so the `with_*` helper's
    /// error-path cleanup cannot mask the caller's original error.
    fn end_read_snapshot(&self) -> crate::Result<()> {
        Ok(())
    }
}

/// Run `f` against `overlay` with a read snapshot held for its duration.
///
/// Mirror of [`crate::structure::graph::with_graph_read_snapshot`]. See that
/// function for the rationale; the overlay has the same multi-query read
/// consistency hazard when writers (commentary refresh, orphan pruning)
/// commit mid-request.
pub fn with_overlay_read_snapshot<F, R>(overlay: &dyn OverlayStore, f: F) -> crate::Result<R>
where
    F: FnOnce(&dyn OverlayStore) -> crate::Result<R>,
{
    overlay.begin_read_snapshot()?;
    let result = f(overlay);
    if let Err(err) = overlay.end_read_snapshot() {
        tracing::debug!(error = %err, "overlay end_read_snapshot failed; ignoring");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{FileNodeId, SymbolNodeId};

    fn sample_link(
        from: NodeId,
        to: NodeId,
        from_hash: &str,
        to_hash: &str,
        pass_id: &str,
    ) -> OverlayLink {
        OverlayLink {
            from,
            to,
            kind: OverlayEdgeKind::References,
            epistemic: OverlayEpistemic::MachineAuthoredHighConf,
            source_spans: vec![CitedSpan {
                artifact: from,
                normalized_text: "authenticate".into(),
                verified_at_offset: 0,
                lcs_ratio: 1.0,
            }],
            target_spans: vec![CitedSpan {
                artifact: to,
                normalized_text: "fn authenticate".into(),
                verified_at_offset: 0,
                lcs_ratio: 1.0,
            }],
            from_content_hash: from_hash.into(),
            to_content_hash: to_hash.into(),
            confidence_score: 0.9,
            confidence_tier: ConfidenceTier::High,
            rationale: None,
            provenance: CrossLinkProvenance {
                pass_id: pass_id.into(),
                model_identity: "claude-sonnet-4-6".into(),
                generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
            },
        }
    }

    #[test]
    fn derive_link_freshness_fresh_when_both_hashes_match() {
        let from = NodeId::Concept(crate::core::ids::ConceptNodeId(1));
        let to = NodeId::Symbol(SymbolNodeId(2));
        let link = sample_link(from, to, "h-from", "h-to", "cross-link-v1");
        assert_eq!(
            derive_link_freshness(&link, Some("h-from"), Some("h-to")),
            CrossLinkFreshness::Fresh
        );
    }

    #[test]
    fn derive_link_freshness_stale_when_either_hash_differs() {
        let from = NodeId::File(FileNodeId(10));
        let to = NodeId::Symbol(SymbolNodeId(20));
        let link = sample_link(from, to, "h-from", "h-to", "cross-link-v1");
        assert_eq!(
            derive_link_freshness(&link, Some("h-from"), Some("h-to-new")),
            CrossLinkFreshness::Stale
        );
        assert_eq!(
            derive_link_freshness(&link, Some("h-from-new"), Some("h-to")),
            CrossLinkFreshness::Stale
        );
    }

    #[test]
    fn derive_link_freshness_source_deleted_when_endpoint_missing() {
        let from = NodeId::File(FileNodeId(10));
        let to = NodeId::Symbol(SymbolNodeId(20));
        let link = sample_link(from, to, "h-from", "h-to", "cross-link-v1");
        assert_eq!(
            derive_link_freshness(&link, None, Some("h-to")),
            CrossLinkFreshness::SourceDeleted
        );
        assert_eq!(
            derive_link_freshness(&link, Some("h-from"), None),
            CrossLinkFreshness::SourceDeleted
        );
    }

    #[test]
    fn derive_link_freshness_invalid_on_empty_provenance_fields() {
        let from = NodeId::File(FileNodeId(10));
        let to = NodeId::Symbol(SymbolNodeId(20));
        let mut link = sample_link(from, to, "h-from", "h-to", "cross-link-v1");
        link.provenance.model_identity = String::new();
        assert_eq!(
            derive_link_freshness(&link, Some("h-from"), Some("h-to")),
            CrossLinkFreshness::Invalid
        );
    }

    #[test]
    fn classify_confidence_partitions_the_score_range() {
        let t = ConfidenceThresholds::default();
        assert_eq!(classify_confidence(0.95, t), ConfidenceTier::High);
        assert_eq!(classify_confidence(0.85, t), ConfidenceTier::High);
        assert_eq!(classify_confidence(0.7, t), ConfidenceTier::ReviewQueue);
        assert_eq!(classify_confidence(0.6, t), ConfidenceTier::ReviewQueue);
        assert_eq!(classify_confidence(0.59, t), ConfidenceTier::BelowThreshold);
        assert_eq!(classify_confidence(0.0, t), ConfidenceTier::BelowThreshold);
    }

    #[test]
    fn stable_identifiers_round_trip_through_serde() {
        for (tier, label) in [
            (ConfidenceTier::High, "high"),
            (ConfidenceTier::ReviewQueue, "review_queue"),
            (ConfidenceTier::BelowThreshold, "below_threshold"),
        ] {
            assert_eq!(tier.as_str(), label);
            let json = serde_json::to_string(&tier).unwrap();
            assert_eq!(json, format!("\"{label}\""));
        }
        for (state, label) in [
            (CrossLinkFreshness::Fresh, "fresh"),
            (CrossLinkFreshness::Stale, "stale"),
            (CrossLinkFreshness::SourceDeleted, "source_deleted"),
            (CrossLinkFreshness::Invalid, "invalid"),
            (CrossLinkFreshness::Missing, "missing"),
        ] {
            assert_eq!(state.as_str(), label);
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, format!("\"{label}\""));
        }
        for (state, label) in [
            (CrossLinkState::Active, "active"),
            (CrossLinkState::Promoted, "promoted"),
            (CrossLinkState::Rejected, "rejected"),
        ] {
            assert_eq!(state.as_str(), label);
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, format!("\"{label}\""));
        }
    }
}
