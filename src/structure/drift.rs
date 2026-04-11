//! Drift scoring for graph edges.
//!
//! The structural pipeline computes a drift score in `[0.0, 1.0]` for
//! every edge on every commit. A low score means the edge is fresh;
//! a high score means the linked source artifacts have diverged
//! structurally since the edge was created.
//!
//! Drift scoring catches prose rot when linked code changes structurally.
//! It does NOT catch cases where code stays structurally similar while
//! meaning changes — phase 6 may add a "decision relevance decay" signal
//! for those cases. See `synrepo-design-v4.md` "Unresolved risks".

use crate::structure::graph::Edge;

/// Compute a drift score for an edge based on structural changes to the
/// source artifacts since the edge was created.
///
/// Phase 1 scoring (conservative):
/// - Both artifacts unchanged since edge creation → 0.0
/// - Only cosmetic changes (whitespace, comments, formatting) → 0.0–0.1
/// - Signature changes but body preserved → 0.1–0.3
/// - Body changes but signature preserved → 0.3–0.6
/// - Signature and body both changed → 0.6–0.9
/// - One artifact deleted → 1.0 (mark for cleanup)
pub fn compute_drift_score(_edge: &Edge) -> f32 {
    // TODO(phase-1): implement.
    0.0
}
