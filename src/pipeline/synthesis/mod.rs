//! The synthesis pipeline.
//!
//! Cold path. LLM-driven. Optional.
//! Produces commentary, proposed cross-links, and findings.
//!
//! This module defines the [`CommentaryGenerator`] trait: the narrow,
//! LLM-provider-agnostic boundary between the card compiler (and the
//! `refresh_commentary` repair action) and the model that actually produces
//! commentary text. Implementations:
//!
//! - [`stub::NoOpGenerator`]: always returns `Ok(None)`. Used when no API
//!   key is configured, inside tests, and as a fallback when the live
//!   generator is disabled.
//! - [`claude::ClaudeCommentaryGenerator`]: calls the Claude Messages API
//!   when `SYNREPO_ANTHROPIC_API_KEY` is set.

pub mod claude;
pub mod cross_link;
pub mod cross_link_claude;
pub mod stub;

pub use claude::ClaudeCommentaryGenerator;
pub use cross_link::{
    score, CandidatePair, CandidateScope, CrossLinkGenerator, NoOpCrossLinkGenerator,
};
pub use cross_link_claude::ClaudeCrossLinkGenerator;
pub use stub::NoOpGenerator;

use crate::core::ids::NodeId;
use crate::overlay::CommentaryEntry;

/// Narrow boundary between the card compiler and an LLM-backed commentary
/// producer.
///
/// `generate` is called lazily: only when a card at `Deep` budget has no
/// matching overlay entry. `context` is the structural card text passed as
/// the input prompt. Implementations SHOULD return `Ok(None)` rather than
/// an error when generation is skipped (no key, budget exhausted, etc.) so
/// the caller can cleanly treat the result as `FreshnessState::Missing`.
///
/// Note: implementations populate `pass_id`, `model_identity`, and
/// `generated_at`. The caller sets `source_content_hash` from the graph
/// before persisting, so returned entries may carry an empty hash.
pub trait CommentaryGenerator: Send + Sync {
    /// Produce a commentary entry for a node.
    fn generate(&self, node: NodeId, context: &str) -> crate::Result<Option<CommentaryEntry>>;
}
