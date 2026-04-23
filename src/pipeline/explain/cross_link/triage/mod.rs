//! Triage prefilter submodules.
//!
//! - `deterministic` — name-match and graph-distance filtering
//! - `semantic` — embedding-based filtering (requires `semantic-triage` feature)

use crate::core::ids::NodeId;

pub mod deterministic;
pub mod semantic;

// Re-exports
pub use deterministic::{candidate_pairs, DEFAULT_DISTANCE_CUTOFF, MIN_IDENT_LEN};
// `semantic_candidates` is gated because `semantic.rs` is `#![cfg(feature = "semantic-triage")]`
// at the file level; the symbol does not exist when the feature is off.
#[cfg(feature = "semantic-triage")]
pub use semantic::semantic_candidates;

/// Triage scope driving `candidate_pairs`. The caller supplies the concept
/// nodes it wants to consider; a production call runs against all concepts.
#[derive(Clone, Debug)]
pub struct TriageScope {
    /// Concept nodes whose prose is mined for identifier mentions.
    pub concepts: Vec<NodeId>,
    /// Maximum graph distance between a concept and a candidate symbol.
    pub distance_cutoff: u32,
}

impl Default for TriageScope {
    fn default() -> Self {
        Self {
            concepts: Vec::new(),
            distance_cutoff: DEFAULT_DISTANCE_CUTOFF,
        }
    }
}
