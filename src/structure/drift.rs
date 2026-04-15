//! Drift scoring for graph edges (stage 7 — scaffold only, not yet wired).
//!
//! The eventual contract: each edge carries a drift score in `[0.0, 1.0]`,
//! where a low score means the edge is fresh and a high score means the
//! linked source artifacts have diverged structurally since the edge was
//! created. The current implementation is a stub: [`compute_drift_score`]
//! always returns `0.0`. See `AGENTS.md` "Phase status" for the live
//! pipeline-stage status, and `synrepo-design-v4.md` "Unresolved risks"
//! for the broader drift-vs-meaning trade-off this stage will eventually
//! address.

use crate::structure::graph::Edge;

/// Compute a drift score for an edge. **Stub** — currently always returns
/// `0.0`. The intended scoring once stage 7 lands:
///
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
