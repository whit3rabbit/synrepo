//! File and symbol identity resolution.
//!
//! The AST-based rename detection cascade. See `synrepo-design-v4.md`
//! section "Identity and stability" for the full algorithm. Phase 1 stub.

use crate::structure::graph::{FileNode, GraphStore};
use crate::core::ids::FileNodeId;

/// Result of the rename detection cascade for one "disappeared" file.
#[derive(Clone, Debug)]
pub enum IdentityResolution {
    /// Exact rename: one new file has substantially overlapping symbols.
    /// Preserve the old node ID, append the new path to path history.
    Rename {
        /// The file node ID to preserve.
        preserved_id: FileNodeId,
        /// The new path to append.
        new_path: String,
    },
    /// Split: the symbols are distributed across multiple new files.
    /// Preserve the old node ID on the largest-overlap new file and create
    /// new nodes for the rest with `split_from` provenance edges.
    Split {
        /// The preserved node ID and its new path.
        primary: (FileNodeId, String),
        /// Additional new paths to create as new nodes with `split_from` edges.
        secondaries: Vec<String>,
    },
    /// Merge: multiple old files' symbols are in one new file.
    /// Create a new node and add `merged_from` edges from the old nodes.
    Merge {
        /// The new file's path.
        new_path: String,
        /// Old file IDs to mark as merged into the new one.
        old_ids: Vec<FileNodeId>,
    },
    /// Git rename fallback: symbol evidence was inconclusive but
    /// `git log --follow` identified a rename.
    GitRename {
        /// The file node ID to preserve.
        preserved_id: FileNodeId,
        /// The new path to append.
        new_path: String,
    },
    /// No identity could be resolved. Treat as delete + add.
    Breakage {
        /// The old file ID that lost its home.
        orphaned: FileNodeId,
        /// Reason for logging.
        reason: String,
    },
}

/// Run the rename detection cascade for one compile cycle.
///
/// Given the set of files that disappeared and the set of new files in this
/// cycle, resolve identities using (1) AST symbol-set match, (2) split,
/// (3) merge, (4) git rename, (5) breakage.
pub fn resolve_identities(
    _disappeared: &[FileNode],
    _new_files: &[FileNode],
    _graph: &dyn GraphStore,
) -> crate::Result<Vec<IdentityResolution>> {
    // TODO(phase-1): implement the full cascade.
    Ok(Vec::new())
}