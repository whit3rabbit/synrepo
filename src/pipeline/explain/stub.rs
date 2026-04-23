//! No-op commentary generator.
//!
//! Used when no LLM key is configured, inside tests, and as the default
//! generator when a caller does not want live generation.

use crate::core::ids::NodeId;
use crate::overlay::CommentaryEntry;

use super::CommentaryGenerator;

/// A generator that never produces an entry.
///
/// `generate` always returns `Ok(None)`, so the caller cleanly falls back
/// to `FreshnessState::Missing` without logging an error.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpGenerator;

impl CommentaryGenerator for NoOpGenerator {
    fn generate(&self, _node: NodeId, _context: &str) -> crate::Result<Option<CommentaryEntry>> {
        Ok(None)
    }
}
