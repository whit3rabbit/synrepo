//! Stable identifier types for graph nodes and edges.
//!
//! Identity stability is the single most important correctness property in
//! synrepo. File node identity survives renames via AST-based detection
//! (see [`crate::identity`]); symbol node identity is keyed on
//! `(file_node_id, qualified_name, kind, body_hash)`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable identifier for a file node in the graph.
///
/// Derived from the content hash of the first version synrepo ever saw for a
/// given content. Survives renames through AST-based detection. On a rename,
/// the node ID is preserved and a new path entry is appended to the file's
/// path history.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FileNodeId(pub u64);

impl fmt::Display for FileNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file_{:016x}", self.0)
    }
}

/// Stable identifier for a symbol node in the graph.
///
/// Derived from `(file_node_id, qualified_name, kind, body_hash)`. The body
/// hash means a symbol whose body is rewritten gets a new identity revision
/// but keeps its logical identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SymbolNodeId(pub u64);

impl fmt::Display for SymbolNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sym_{:016x}", self.0)
    }
}

/// Stable identifier for a concept node in the graph.
///
/// Concept nodes are only created from human-authored Markdown files in
/// configured concept directories. In auto mode, if no concept directories
/// exist, there are no ConceptNodeIds in the graph at all — and that's fine,
/// because cards cover the common case without needing an ontology layer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ConceptNodeId(pub u64);

impl fmt::Display for ConceptNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "concept_{:016x}", self.0)
    }
}

/// Unified node ID. Used in graph edges and MCP responses where the node
/// type is determined at runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum NodeId {
    /// A file node.
    File(FileNodeId),
    /// A symbol node.
    Symbol(SymbolNodeId),
    /// A concept node.
    Concept(ConceptNodeId),
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeId::File(id) => write!(f, "{}", id),
            NodeId::Symbol(id) => write!(f, "{}", id),
            NodeId::Concept(id) => write!(f, "{}", id),
        }
    }
}

/// Stable identifier for a graph edge.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct EdgeId(pub u64);

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "edge_{:016x}", self.0)
    }
}