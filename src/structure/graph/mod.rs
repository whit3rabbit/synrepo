//! The canonical graph: nodes and edges that were directly observed by
//! parsers, git, or humans. Machine-authored content does not exist in
//! this layer — it lives in [`crate::overlay`].

mod edge;
mod epistemic;
mod node;
mod store;

pub use edge::{Edge, EdgeKind};
pub use epistemic::Epistemic;
pub use node::{FileNode, SymbolKind, SymbolNode};
pub use store::GraphStore;
