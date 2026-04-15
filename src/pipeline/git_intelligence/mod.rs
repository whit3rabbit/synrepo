//! Deterministic, non-canonical Git-intelligence preparation.
//!
//! This module is the intended entry point for future history-mining work.
//! It consumes the typed pipeline Git context instead of opening `gix`
//! directly, which keeps degraded-history handling and config coupling in one place.

mod analysis;
mod emit;
mod index;
mod symbol_revisions;
mod types;

#[cfg(test)]
mod tests;

pub use analysis::{analyze_path_history, analyze_recent_history, sample_recent_history};
pub use emit::emit_cochange_edges;
pub use index::GitHistoryIndex;
pub use symbol_revisions::derive_symbol_revisions;
pub use types::{
    GitCoChange, GitFileHotspot, GitHistoryInsights, GitHistorySample, GitIntelligenceStatus,
    GitOwnershipHint, GitPathCoChangePartner, GitPathHistoryInsights,
};
