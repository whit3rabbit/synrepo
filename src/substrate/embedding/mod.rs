//! Embedding-based semantic index for cross-link candidate prefiltering.
//!
//! This module provides:
//! - Chunk extraction from graph symbols and prose concepts
//! - Embedding model resolution and downloading (global cache)
//! - ONNX inference using `ort` and `tokenizers`
//! - Flat vector index with dot-product similarity search
//!
//! All functionality is gated behind the `semantic-triage` feature flag.

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
use crate::structure::graph::GraphStore;

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
/// Returns None if semantic triage is not enabled.
/// If enabled but the index or model is missing/invalid, returns an Error (strict policy).
#[cfg(feature = "semantic-triage")]
pub fn load_embedding_index(
    config: &Config,
    synrepo_dir: &std::path::Path,
) -> Result<Option<FlatVecIndex>> {
    if !config.enable_semantic_triage {
        return Ok(None);
    }

    let index_path = synrepo_dir.join("index/vectors/index.bin");
    if !index_path.exists() {
        // If config is enabled but index is missing, it's an error in strict mode
        return Err(crate::Error::Other(anyhow::anyhow!(
            "Semantic triage is enabled but embedding index is missing at {}. Run 'synrepo reconcile' to build it.",
            index_path.display()
        )));
    }

    // Resolve the model from global cache (strict failure if missing/invalid)
    let resolver = ModelResolver::new();
    let model_res = resolver.resolve(&config.semantic_model, synrepo_dir, config.embedding_dim)?;

    // Load the index with the model session restored
    let index = index::FlatVecIndex::load_with_resolution(&index_path, config.embedding_dim, &model_res)?;

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

    // Resolve and load the model (shared global cache)
    let resolver = ModelResolver::new();
    let model = resolver.resolve(&config.semantic_model, synrepo_dir, config.embedding_dim)?;

    // Extract chunks from the graph
    let chunks = chunk::extract_chunks(graph)?;

    // Build embeddings for all chunks (performs real inference and normalization)
    let index = index::FlatVecIndex::build(chunks, model)?;

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
