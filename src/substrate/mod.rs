//! The substrate layer: file discovery, classification, and lexical indexing.
//!
//! Spec: `openspec/specs/substrate/spec.md`

pub mod classify;
pub mod discover;
pub mod hybrid;
pub mod incremental;
pub mod index;

pub mod embedding;

pub use classify::{classify, FileClass, SkipReason};
pub use discover::{discover, discover_roots, DiscoveredFile, DiscoveryRoot, DiscoveryRootKind};
pub use hybrid::{hybrid_search, HybridSearchReport, HybridSearchRow, HybridSearchSource};
pub use incremental::{sync_index_incremental, IndexSyncMode, IndexSyncReport};
pub use index::{build_index, search, search_with_options, IndexBuildReport};

#[cfg(feature = "semantic-triage")]
pub use embedding::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};
#[cfg(feature = "semantic-triage")]
pub use embedding::{build_embedding_index, is_available, load_embedding_index, FlatVecIndex};
