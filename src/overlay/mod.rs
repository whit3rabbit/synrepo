//! The overlay store: LLM-authored content, physically separate from the graph.
//!
//! Defines the [`OverlayStore`] trait and its payload types: commentary
//! entries (with provenance, freshness derivation, and staleness repair), plus
//! cross-link proposals (stubbed; activated in phase 5+).
//!
//! The key invariant: the explain pipeline never reads its own previous
//! output as retrieval input. This is enforced both physically (overlay
//! lives in a separate sqlite database from the graph) and at the retrieval
//! layer (explain queries filter on `source_store = "graph"`).

pub mod agent_notes;
pub mod commentary;
pub mod types;

pub use agent_notes::*;
pub use commentary::*;
pub use types::*;

use crate::core::ids::NodeId;
use crate::pipeline::maintenance::{CompactPolicy, CompactStats};

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

    /// Insert an advisory agent note and append its creation transition.
    fn insert_note(&mut self, note: AgentNote) -> crate::Result<AgentNote>;

    /// Query advisory notes. Normal queries hide forgotten, superseded, and invalid notes.
    fn query_notes(&self, query: AgentNoteQuery) -> crate::Result<Vec<AgentNote>>;

    /// Return one note by ID.
    fn note_by_id(&self, note_id: &str) -> crate::Result<Option<AgentNote>>;

    /// Link two notes without changing either claim.
    fn link_note(&mut self, from_note: &str, to_note: &str, actor: &str) -> crate::Result<()>;

    /// Supersede an existing note with a replacement note.
    fn supersede_note(
        &mut self,
        old_note: &str,
        replacement: AgentNote,
        actor: &str,
    ) -> crate::Result<AgentNote>;

    /// Hide a note from normal retrieval while retaining audit history.
    fn forget_note(
        &mut self,
        note_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> crate::Result<()>;

    /// Verify a note against current source-derived facts.
    fn verify_note(
        &mut self,
        note_id: &str,
        actor: &str,
        graph_revision: Option<u64>,
    ) -> crate::Result<AgentNote>;

    /// Mark notes stale when their drift anchors no longer match.
    fn mark_stale_notes(&mut self, stale_note_ids: &[String], actor: &str) -> crate::Result<usize>;

    /// Return note counts by lifecycle status.
    fn note_counts(&self) -> crate::Result<AgentNoteCounts>;

    /// Delete all commentary entries and cross-link candidates whose endpoint
    /// node IDs are not present in `live_nodes`. Pruned cross-links leave an
    /// immutable audit row. Returns the total number of rows deleted.
    fn prune_orphans(&mut self, live_nodes: &[NodeId]) -> crate::Result<usize>;

    /// Retrieve all active candidates, optionally filtered by tier.
    fn all_candidates(&self, tier: Option<&str>) -> crate::Result<Vec<OverlayLink>>;

    /// Look up a single cross-link candidate by its `(from, to, kind)` triple.
    /// Used by the revalidation handler before calling the fuzzy-LCS verifier.
    fn candidate_by_endpoints(
        &self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
    ) -> crate::Result<Option<OverlayLink>>;

    /// Replace a candidate's stored endpoint hashes and verified spans after
    /// the fuzzy-LCS verifier re-locates the cited text against current source.
    /// Preserves state, tier, reviewer, and promotion columns.
    #[allow(clippy::too_many_arguments)]
    fn revalidate_link(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        new_from_hash: &str,
        new_to_hash: &str,
        new_source_spans: &[CitedSpan],
        new_target_spans: &[CitedSpan],
    ) -> crate::Result<()>;

    /// Mark a candidate as rejected by a human reviewer.
    fn mark_candidate_rejected(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()>;

    /// Mark a candidate as pending promotion. This acts as an atomicity bridge:
    /// if the process crashes after this but before `mark_candidate_promoted`,
    /// the system can detect the inconsistent state.
    fn mark_candidate_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()>;

    /// Mark a candidate as promoted into the graph.
    fn mark_candidate_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
        graph_edge_id: &str,
    ) -> crate::Result<()>;

    /// Return compactable statistics for commentary entries older than policy retention.
    fn compactable_commentary_stats(&self, policy: &CompactPolicy) -> crate::Result<CompactStats>;

    /// Compact stale commentary entries, returning count deleted.
    fn compact_commentary(&mut self, policy: &CompactPolicy) -> crate::Result<usize>;

    /// Return compactable statistics for cross-link audit rows older than policy retention.
    fn compactable_cross_link_stats(&self, policy: &CompactPolicy) -> crate::Result<CompactStats>;

    /// Compact old cross-link audit rows, returning count deleted/summarized.
    fn compact_cross_links(&mut self, policy: &CompactPolicy) -> crate::Result<usize>;

    /// Return the count of audit rows in the cross_link_audit table.
    fn cross_link_audit_count(&self) -> crate::Result<usize>;

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

/// RAII guard that ends the overlay read snapshot on drop, including on panic
/// unwind. Why: without this, a panic in the `with_overlay_read_snapshot`
/// closure would skip `end_read_snapshot` and leave a `BEGIN DEFERRED`
/// transaction open on the overlay connection.
struct OverlaySnapshotGuard<'a>(&'a dyn OverlayStore);

impl Drop for OverlaySnapshotGuard<'_> {
    fn drop(&mut self) {
        if let Err(err) = self.0.end_read_snapshot() {
            tracing::warn!(error = %err, "overlay end_read_snapshot failed; ignoring");
        }
    }
}

/// Run `f` against `overlay` with a read snapshot held for its duration.
///
/// Mirror of [`crate::structure::graph::with_graph_read_snapshot`]. See that
/// function for the rationale; the overlay has the same multi-query read
/// consistency hazard when writers (commentary refresh, orphan pruning)
/// commit mid-request. Snapshot end is structural via [`OverlaySnapshotGuard`]
/// so panic in `f` still ends the snapshot.
pub fn with_overlay_read_snapshot<F, R>(overlay: &dyn OverlayStore, f: F) -> crate::Result<R>
where
    F: FnOnce(&dyn OverlayStore) -> crate::Result<R>,
{
    overlay.begin_read_snapshot()?;
    let _guard = OverlaySnapshotGuard(overlay);
    f(overlay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{FileNodeId, SymbolNodeId};
    use time::OffsetDateTime;

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
