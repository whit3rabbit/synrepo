//! The canonical graph: nodes and edges that were directly observed by
//! parsers, git, or humans. Machine-authored content does not exist in
//! this layer — it lives in [`crate::overlay`].

mod edge;
mod epistemic;
mod in_memory;
mod mem_store;
mod node;
/// Process-global in-memory graph snapshot accessors.
pub mod snapshot;
mod store;

#[cfg(test)]
mod tests;

pub use edge::{derive_edge_id, Edge, EdgeKind};
pub use epistemic::Epistemic;
pub use in_memory::Graph;
pub use mem_store::MemGraphStore;
pub use node::{
    concept_source_path_allowed, ConceptNode, FileNode, SymbolKind, SymbolNode, Visibility,
};
pub use store::{with_graph_read_snapshot, CompactionSummary, GraphReader, GraphStore};
