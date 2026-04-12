//! Shared primitives for the commentary overlay repair surface.
//!
//! `check`, `sync`, and the `status` coverage line all walk the overlay's
//! commentary table, parse each `NodeId`, and look up its current content
//! hash (plus metadata) from the graph. This module owns that resolution
//! step so the three call sites stay in sync.

use crate::core::ids::NodeId;
use crate::structure::graph::{FileNode, GraphStore, SymbolNode};

/// Snapshot of the graph-side node a commentary entry points at.
///
/// `content_hash` is the hash used to classify freshness. `file` is the file
/// the commentary is ultimately tied to (for symbol commentary, the
/// containing file). `symbol` is present only for symbol commentary.
pub struct CommentaryNodeSnapshot {
    /// Content hash compared against the stored `source_content_hash` to
    /// decide freshness. Always the containing file's hash.
    pub content_hash: String,
    /// The file node the commentary is ultimately tied to. For symbol
    /// commentary this is the containing file.
    pub file: FileNode,
    /// Present only for symbol commentary; `None` for file commentary.
    pub symbol: Option<SymbolNode>,
}

/// Resolve the graph-side snapshot for a commentary target.
///
/// Returns `Ok(None)` when the node no longer exists or is a kind that has
/// no content hash to compare against (concepts). Propagates graph-store
/// errors.
pub fn resolve_commentary_node(
    graph: &dyn GraphStore,
    node: NodeId,
) -> crate::Result<Option<CommentaryNodeSnapshot>> {
    match node {
        NodeId::File(file_id) => Ok(graph.get_file(file_id)?.map(|file| CommentaryNodeSnapshot {
            content_hash: file.content_hash.clone(),
            file,
            symbol: None,
        })),
        NodeId::Symbol(sym_id) => {
            let Some(symbol) = graph.get_symbol(sym_id)? else {
                return Ok(None);
            };
            let Some(file) = graph.get_file(symbol.file_id)? else {
                return Ok(None);
            };
            Ok(Some(CommentaryNodeSnapshot {
                content_hash: file.content_hash.clone(),
                file,
                symbol: Some(symbol),
            }))
        }
        // Concept nodes do not track a content hash, so commentary against
        // them cannot be classified as fresh/stale here.
        NodeId::Concept(_) => Ok(None),
    }
}
