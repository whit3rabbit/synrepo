//! No-op commentary generator.
//!
//! Used when no LLM key is configured, inside tests, and as the default
//! generator when a caller does not want live generation.

use crate::core::ids::NodeId;
use crate::overlay::CommentaryEntry;

use super::{
    CommentaryFuture, CommentaryGeneration, CommentaryGenerator, CommentarySkip,
    CommentarySkipReason,
};

/// A generator that never produces an entry.
///
/// `generate` always returns `Ok(None)`, so the caller cleanly falls back
/// to `FreshnessState::Missing` without logging an error.
#[derive(Clone, Copy, Debug)]
pub struct NoOpGenerator {
    reason: CommentarySkipReason,
}

impl NoOpGenerator {
    /// No-op because explain is not enabled.
    pub fn provider_disabled() -> Self {
        Self {
            reason: CommentarySkipReason::ProviderDisabled,
        }
    }

    /// No-op because the selected provider lacks credentials.
    pub fn missing_api_key() -> Self {
        Self {
            reason: CommentarySkipReason::MissingApiKey,
        }
    }
}

impl Default for NoOpGenerator {
    fn default() -> Self {
        Self::provider_disabled()
    }
}

impl CommentaryGenerator for NoOpGenerator {
    fn generate(&self, _node: NodeId, _context: &str) -> crate::Result<Option<CommentaryEntry>> {
        Ok(None)
    }

    fn generate_with_outcome(
        &self,
        _node: NodeId,
        _context: &str,
    ) -> crate::Result<CommentaryGeneration> {
        Ok(CommentaryGeneration::Skipped(CommentarySkip::new(
            self.reason,
        )))
    }

    fn generate_with_outcome_async<'a>(
        &'a self,
        _node: NodeId,
        _context: &'a str,
    ) -> CommentaryFuture<'a> {
        Box::pin(async move {
            Ok(CommentaryGeneration::Skipped(CommentarySkip::new(
                self.reason,
            )))
        })
    }
}
