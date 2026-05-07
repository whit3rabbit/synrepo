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
pub mod build;
#[cfg(feature = "semantic-triage")]
pub mod chunk;
#[cfg(feature = "semantic-triage")]
pub mod index;
#[cfg(feature = "semantic-triage")]
pub mod model;

#[cfg(feature = "semantic-triage")]
pub use build::{build_embedding_index_with_progress, EmbeddingBuildEvent, EmbeddingBuildSummary};
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

/// Explicit embedding builds require the optional semantic-triage feature.
#[cfg(not(feature = "semantic-triage"))]
pub fn build_embedding_index_with_progress(
    _graph: &dyn crate::structure::graph::GraphStore,
    _config: &Config,
    _synrepo_dir: &std::path::Path,
    _progress: Option<&mut dyn FnMut(EmbeddingBuildEvent)>,
    _should_stop: Option<&mut dyn FnMut() -> bool>,
) -> Result<EmbeddingBuildSummary> {
    Err(crate::Error::Other(anyhow::anyhow!(
        "embeddings are optional; this binary was not built with `semantic-triage`"
    )))
}

/// Progress event placeholder used when semantic triage is not compiled.
#[cfg(not(feature = "semantic-triage"))]
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EmbeddingBuildEvent {
    /// Model/provider resolution started.
    ResolvingModel {
        /// Embedding backend, for example `onnx` or `ollama`.
        provider: String,
        /// Configured model name.
        model: String,
        /// Expected vector dimension.
        dim: u16,
    },
    /// Model/provider is resolved and ready to initialize.
    ModelReady {
        /// Embedding backend, for example `onnx` or `ollama`.
        provider: String,
        /// Resolved model name.
        model: String,
        /// Expected vector dimension.
        dim: u16,
        /// Whether ONNX artifacts were downloaded during resolution.
        downloaded: bool,
    },
    /// Runtime session initialization started.
    InitializingBackend,
    /// One small provider request/inference is starting.
    PreflightStarted,
    /// Provider preflight completed successfully.
    PreflightFinished,
    /// Chunk extraction from the graph started.
    ExtractingChunks,
    /// Chunks are ready for embedding.
    ChunksReady {
        /// Number of chunks that will be embedded.
        chunks: usize,
    },
    /// A batch of chunks finished embedding.
    BatchFinished {
        /// Number of chunks embedded so far.
        current: usize,
        /// Total chunks planned for this build.
        total: usize,
    },
    /// Persisting the index started.
    SavingIndex {
        /// Destination index path.
        path: std::path::PathBuf,
    },
    /// Build completed and the index is persisted.
    Finished {
        /// Number of chunks written to the index.
        chunks: usize,
        /// Destination index path.
        path: std::path::PathBuf,
        /// Resolved model name.
        model: String,
        /// Embedding backend, for example `onnx` or `ollama`.
        provider: String,
    },
}

/// Build summary placeholder used when semantic triage is not compiled.
#[cfg(not(feature = "semantic-triage"))]
#[derive(Clone, Debug, serde::Serialize)]
pub struct EmbeddingBuildSummary {
    /// Embedding backend.
    pub provider: String,
    /// Model name.
    pub model: String,
    /// Vector dimension.
    pub dim: u16,
    /// Number of chunks written.
    pub chunks: usize,
    /// Destination index path.
    pub index_path: std::path::PathBuf,
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
            "Semantic triage is enabled but embedding index is missing at {}. Run 'synrepo embeddings build' to build it.",
            index_path.display()
        )));
    }

    // Resolve the model from global cache without downloading. Query-time
    // semantic availability must stay local-only.
    let resolver = ModelResolver::new();
    let model_res = resolver.resolve_existing(config, synrepo_dir)?;

    // Load the index with the model session restored
    let index =
        index::FlatVecIndex::load_with_resolution(&index_path, config.embedding_dim, &model_res)?;

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
    let model = resolver.resolve(config, synrepo_dir)?;

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

#[cfg(all(test, feature = "semantic-triage"))]
mod tests;
