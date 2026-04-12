//! The substrate layer: file discovery, classification, and lexical indexing.
//!
//! Spec: `openspec/specs/substrate/spec.md`

pub mod classify;
pub mod discover;
pub mod index;

pub use classify::{classify, FileClass, SkipReason};
pub use discover::{discover, DiscoveredFile};
pub use index::{build_index, search, search_with_options, IndexBuildReport};
