//! Embedding-based semantic index for cross-link candidate prefiltering.
//!
//! This module provides:
//! - Chunk extraction from graph symbols and prose concepts
//! - Embedding model resolution and downloading
//! - ONNX inference using `ort`
//! - Flat vector index with cosine similarity search
//!
//! All functionality is gated behind the `semantic-triage` feature flag.
//!
//! Spec: `openspec/changes/semantic-triage-v1/specs/semantic-triage/spec.md`

#[cfg(feature = "semantic-triage")]
pub mod chunk;
#[cfg(feature = "semantic-triage")]
pub mod index;
#[cfg(feature = "semantic-triage")]
pub mod model;

#[cfg(feature = "semantic-triage")]
pub use chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};
#[cfg(feature = "semantic-triage")]
pub use index::FlatVecIndex;
#[cfg(feature = "semantic-triage")]
pub use model::{ModelResolution, ModelResolver};

use crate::config::Config;
use crate::Result;

#[cfg(feature = "semantic-triage")]
use crate::structure::graph::{with_graph_read_snapshot, GraphStore};

/// Build the embedding index for a graph store if semantic triage is enabled.
#[cfg(feature = "semantic-triage")]
pub fn build_embedding_index<G: GraphStore>(
    graph: &G,
    config: &Config,
    synrepo_dir: &std::path::Path,
) -> Result<Option<FlatVecIndex>> {
    if !config.enable_semantic_triage {
        return Ok(None);
    }

    let index = build_index_with_config(graph, config, synrepo_dir)?;
    Ok(Some(index))
}

/// Load an existing embedding index for query-time use.
/// Returns None if semantic triage is not enabled or index doesn't exist.
#[cfg(feature = "semantic-triage")]
pub fn load_embedding_index(
    config: &Config,
    synrepo_dir: &std::path::Path,
) -> Result<Option<FlatVecIndex>> {
    if !config.enable_semantic_triage {
        return Ok(None);
    }

    let vectors_dir = synrepo_dir.join("index/vectors");
    let index_path = vectors_dir.join("index.bin");
    let model_path = vectors_dir.join("model.onnx");

    if !index_path.exists() {
        return Ok(None);
    }

    // Load the index with the model for query-time embedding
    let index = if model_path.exists() {
        index::FlatVecIndex::load_with_model_path(&index_path, config.embedding_dim, &model_path)?
    } else {
        // Model not present, load without session (can't embed queries)
        index::FlatVecIndex::load(&index_path, config.embedding_dim)?
    };

    Ok(Some(index))
}

#[cfg(feature = "semantic-triage")]
fn build_index_with_config<G: GraphStore>(
    graph: &G,
    config: &Config,
    synrepo_dir: &std::path::Path,
) -> Result<FlatVecIndex> {
    let vectors_dir = synrepo_dir.join("index/vectors");
    std::fs::create_dir_all(&vectors_dir)?;

    let index_path = vectors_dir.join("index.bin");

    // Try to load existing index
    if index_path.exists() {
        match index::FlatVecIndex::load(&index_path, config.embedding_dim) {
            Ok(index) => {
                tracing::info!(
                    "Loaded existing embedding index with {} chunks",
                    index.len()
                );
                return Ok(index);
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load existing index, rebuilding");
            }
        }
    }

    // Resolve and load the model
    let resolver = ModelResolver::new();
    let model = resolver.resolve(&config.semantic_model, &vectors_dir, config.embedding_dim)?;

    // Extract chunks from the graph
    let chunks = chunk::extract_chunks(graph)?;

    // Build embeddings for all chunks
    let index = index::FlatVecIndex::build(chunks, model, &vectors_dir)?;

    // Save to disk
    index.save(&index_path)?;
    tracing::info!(
        "Built and saved embedding index with {} chunks",
        index.len()
    );

    Ok(index)
}

/// Check if semantic triage is available (feature compiled in).
pub fn is_available() -> bool {
    cfg!(feature = "semantic-triage")
}

/// Build gitignore content for vectors directory (used by bootstrap).
pub fn vectors_gitignore_content() -> &'static str {
    "index/vectors/"
}
