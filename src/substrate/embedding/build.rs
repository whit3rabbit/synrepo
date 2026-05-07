//! Explicit embedding index build flow with progress events.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::Config;
use crate::structure::graph::GraphStore;
use crate::Result;

use super::model::EmbeddingSession;
use super::{chunk, FlatVecIndex, ModelResolution, ModelResolver};

/// Progress event emitted by an explicit embedding index build.
#[derive(Clone, Debug, Serialize)]
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
        path: PathBuf,
    },
    /// Build completed and the index is persisted.
    Finished {
        /// Number of chunks written to the index.
        chunks: usize,
        /// Destination index path.
        path: PathBuf,
        /// Resolved model name.
        model: String,
        /// Embedding backend, for example `onnx` or `ollama`.
        provider: String,
    },
}

/// Summary returned after a successful explicit embedding build.
#[derive(Clone, Debug, Serialize)]
pub struct EmbeddingBuildSummary {
    /// Embedding backend, for example `onnx` or `ollama`.
    pub provider: String,
    /// Resolved model name.
    pub model: String,
    /// Vector dimension.
    pub dim: u16,
    /// Number of chunks written to the index.
    pub chunks: usize,
    /// Destination index path.
    pub index_path: PathBuf,
}

/// Build the embedding index for a graph store if semantic triage is enabled.
pub fn build_embedding_index_with_progress(
    graph: &dyn GraphStore,
    config: &Config,
    synrepo_dir: &Path,
    progress: Option<&mut dyn FnMut(EmbeddingBuildEvent)>,
    should_stop: Option<&mut dyn FnMut() -> bool>,
) -> Result<EmbeddingBuildSummary> {
    if !config.enable_semantic_triage {
        return Err(crate::Error::Config(
            "embeddings are disabled; set enable_semantic_triage = true or press T in the dashboard first".to_string(),
        ));
    }

    let mut noop_progress = |_event: EmbeddingBuildEvent| {};
    let progress = progress.unwrap_or(&mut noop_progress);
    let mut never_stop = || false;
    let should_stop = should_stop.unwrap_or(&mut never_stop);

    progress(EmbeddingBuildEvent::ResolvingModel {
        provider: config.semantic_embedding_provider.as_str().to_string(),
        model: config.semantic_model.clone(),
        dim: config.embedding_dim,
    });
    let model = ModelResolver::new().resolve(config, synrepo_dir)?;
    progress(EmbeddingBuildEvent::ModelReady {
        provider: model.provider_label().to_string(),
        model: model.model_name().to_string(),
        dim: model.embedding_dim(),
        downloaded: model.downloaded(),
    });

    progress(EmbeddingBuildEvent::InitializingBackend);
    let session = EmbeddingSession::new_from_resolution(&model)?;
    progress(EmbeddingBuildEvent::PreflightStarted);
    run_preflight(&session, &model)?;
    progress(EmbeddingBuildEvent::PreflightFinished);

    if should_stop() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "embedding build cancelled"
        )));
    }
    progress(EmbeddingBuildEvent::ExtractingChunks);
    let chunks = chunk::extract_chunks(graph)?;
    progress(EmbeddingBuildEvent::ChunksReady {
        chunks: chunks.len(),
    });

    let index = FlatVecIndex::build_with_session_and_progress(
        chunks,
        &model,
        session,
        |current, total| {
            progress(EmbeddingBuildEvent::BatchFinished { current, total });
        },
        should_stop,
    )?;

    let index_path = synrepo_dir.join("index/vectors/index.bin");
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    progress(EmbeddingBuildEvent::SavingIndex {
        path: index_path.clone(),
    });
    index.save(&index_path)?;
    let summary = EmbeddingBuildSummary {
        provider: model.provider_label().to_string(),
        model: model.model_name().to_string(),
        dim: model.embedding_dim(),
        chunks: index.len(),
        index_path,
    };
    progress(EmbeddingBuildEvent::Finished {
        chunks: summary.chunks,
        path: summary.index_path.clone(),
        model: summary.model.clone(),
        provider: summary.provider.clone(),
    });
    Ok(summary)
}

fn run_preflight(session: &EmbeddingSession, model: &ModelResolution) -> Result<()> {
    let probe = ["synrepo embedding preflight".to_string()];
    let vectors = session.embed(&probe).map_err(|err| {
        crate::Error::Other(anyhow::anyhow!(
            "{} embedding preflight failed for model '{}': {}",
            model.provider_label(),
            model.model_name(),
            err
        ))
    })?;
    match vectors.first() {
        Some(vector) if vector.len() == model.embedding_dim() as usize => Ok(()),
        Some(vector) => Err(crate::Error::Other(anyhow::anyhow!(
            "{} embedding preflight returned dimension {}, expected {}",
            model.provider_label(),
            vector.len(),
            model.embedding_dim()
        ))),
        None => Err(crate::Error::Other(anyhow::anyhow!(
            "{} embedding preflight returned no vectors",
            model.provider_label()
        ))),
    }
}
