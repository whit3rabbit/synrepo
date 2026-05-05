//! Edit-enabled MCP surface.
//!
//! Anchors are short-lived operational state for one MCP server process. They
//! are deliberately separate from graph facts, overlay commentary, and agent
//! notes.

mod anchors;
mod apply;
mod atomic;
mod caps;
mod diagnostics;
mod prepare;
mod runtime;

pub use apply::{handle_apply_anchor_edits, ApplyAnchorEditsParams};
pub use prepare::{handle_prepare_edit_context, PrepareEditContextParams};

pub(crate) use anchors::{anchor_manager, AnchorLine, PreparedAnchorState};

#[cfg(test)]
mod tests;
