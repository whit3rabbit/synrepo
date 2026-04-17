//! Cross-link candidate persistence for `SqliteOverlayStore`.
//!
//! This module owns the `cross_links` table. All mutation paths go through
//! functions here, which also append audit rows for every lifecycle event.
//! Read paths reconstruct `OverlayLink` values from stored columns.
//!
//! Split layout:
//! - `types` — row-level structs surfaced to repair and status callers
//! - `codec` — serialization, validation, and enum ↔ string mappings
//! - `read` — read-only queries
//! - `write` — insert/upsert and revalidation-path mutations
//! - `transitions` — state transitions (reject, pending, promote, rollback)

mod codec;
mod read;
mod transitions;
mod types;
mod write;

pub use types::{CrossLinkHashRow, CrossLinkStateCounts, PendingPromotionRow};

// Re-exported so sibling modules (e.g. `commentary`) can continue to reach
// these helpers via `super::cross_links::<fn>`. `pub(crate)` is required
// because `pub(super) use` on an item imported from a private submodule does
// not propagate visibility to siblings of this module.
pub(crate) use read::{all_candidates, candidates_for_node};
pub(crate) use transitions::{mark_pending, mark_promoted, mark_rejected};
pub(crate) use write::{insert_candidate, prune_orphans};

use rusqlite::Connection;

use crate::core::ids::NodeId;
use crate::overlay::{ConfidenceTier, OverlayEdgeKind, OverlayLink};

use super::SqliteOverlayStore;

impl SqliteOverlayStore {
    /// Return the number of cross-link rows currently stored.
    pub fn cross_link_count(&self) -> crate::Result<usize> {
        let conn = self.conn.lock();
        read::count(&conn)
    }

    /// Return every candidate's endpoint keys and stored hashes.
    pub fn cross_link_hashes(&self) -> crate::Result<Vec<CrossLinkHashRow>> {
        let conn = self.conn.lock();
        read::endpoint_hashes(&conn)
    }

    /// Retrieve all active candidates, optionally filtered by tier.
    pub fn all_candidates(&self, tier: Option<&str>) -> crate::Result<Vec<OverlayLink>> {
        let conn = self.conn.lock();
        read::all_candidates(&conn, tier)
    }

    /// Retrieve active candidates with SQL-side limit applied before
    /// materialization. Use when only a bounded page is needed.
    pub fn candidates_limited(
        &self,
        tier: Option<&str>,
        limit: usize,
    ) -> crate::Result<Vec<OverlayLink>> {
        let conn = self.conn.lock();
        read::candidates_limited(&conn, tier, limit)
    }

    /// Refresh stored endpoint hashes for a candidate after a successful
    /// fuzzy-LCS revalidation.
    pub fn refresh_candidate_hashes(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        new_from_hash: &str,
        new_to_hash: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        write::refresh_hashes(&conn, from, to, kind, new_from_hash, new_to_hash)
    }

    /// Update a candidate's confidence tier. Use after revalidation fails.
    pub fn update_candidate_tier(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        new_tier: ConfidenceTier,
        new_score: f32,
        reason: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        write::update_tier(&conn, from, to, kind, new_tier, new_score, reason)
    }

    /// Mark a candidate as rejected by a human reviewer.
    pub fn mark_candidate_rejected(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        transitions::mark_rejected(&conn, from, to, kind, reviewer)
    }

    /// Mark a candidate as pending promotion (atomicity bridge).
    pub fn mark_candidate_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        transitions::mark_pending(&conn, from, to, kind, reviewer)
    }

    /// Mark a candidate as promoted into the graph. The graph-side write is
    pub fn mark_candidate_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
        graph_edge_id: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        transitions::mark_promoted(&conn, from, to, kind, reviewer, graph_edge_id)
    }

    /// Return all cross-link rows stuck in `pending_promotion` state.
    pub fn pending_promotion_rows(&self) -> crate::Result<Vec<PendingPromotionRow>> {
        let conn = self.conn.lock();
        read::pending_promotion_rows(&conn)
    }

    /// Return counts by state for the cross_links table.
    pub fn cross_link_state_counts(&self) -> crate::Result<CrossLinkStateCounts> {
        let conn = self.conn.lock();
        state_counts(&conn)
    }

    /// Reset a `pending_promotion` row back to `active` so it can be re-accepted.
    pub fn reset_candidate_to_active(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        transitions::reset_pending_to_active(&conn, from, to, kind)
    }
}

fn state_counts(conn: &Connection) -> crate::Result<CrossLinkStateCounts> {
    let mut stmt = conn.prepare("SELECT state, COUNT(*) FROM cross_links GROUP BY state")?;
    let mut counts = CrossLinkStateCounts::default();
    for row in stmt.query_map([], |row| {
        let state: String = row.get(0)?;
        let count: usize = row.get(1)?;
        Ok((state, count))
    })? {
        let (state, count) = row?;
        match state.as_str() {
            "active" => counts.active = count,
            "pending_promotion" => counts.pending_promotion = count,
            "promoted" => counts.promoted = count,
            "rejected" => counts.rejected = count,
            _ => {}
        }
    }
    Ok(counts)
}
