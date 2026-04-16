//! The substrate layer: file discovery, classification, and lexical indexing.
//!
//! Spec: `openspec/specs/substrate/spec.md`

pub mod classify;
pub mod discover;
pub mod index;

#[cfg(feature = "semantic-triage")]
pub mod embedding;

pub use classify::{classify, FileClass, SkipReason};
pub use discover::{discover, DiscoveredFile};
pub use index::{build_index, search, search_with_options, IndexBuildReport};

#[cfg(feature = "semantic-triage")]
pub use embedding::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};
#[cfg(feature = "semantic-triage")]
pub use embedding::{build_embedding_index, is_available, load_embedding_index, FlatVecIndex};
