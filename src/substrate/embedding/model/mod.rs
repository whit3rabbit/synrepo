//! Shared types for the embedding model subsystem.
//!
//! Re-exports types from the `resolution` and `session` sub-modules.

use std::path::PathBuf;

use crate::Result;

/// Supported pooling strategies for transformer outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolingStrategy {
    /// Average pooling over the sequence dimension (honoring attention mask).
    Mean,
    /// Use the [CLS] token output (first vector in sequence).
    Cls,
}

/// Result of resolving an embedding model.
#[derive(Clone, Debug)]
pub enum ModelResolution {
    /// ONNX Runtime-backed model bundle.
    Onnx(OnnxModelResolution),
    /// Ollama `/api/embed` local endpoint.
    Ollama(OllamaModelResolution),
}

impl ModelResolution {
    /// User-facing model name.
    pub fn model_name(&self) -> &str {
        match self {
            Self::Onnx(res) => &res.model_name,
            Self::Ollama(res) => &res.model_name,
        }
    }

    /// Embedding vector dimension.
    pub fn embedding_dim(&self) -> u16 {
        match self {
            Self::Onnx(res) => res.embedding_dim,
            Self::Ollama(res) => res.embedding_dim,
        }
    }

    /// Whether returned vectors should be treated as normalized.
    pub fn normalize(&self) -> bool {
        match self {
            Self::Onnx(res) => res.normalize,
            Self::Ollama(res) => res.normalize,
        }
    }
}

/// Resolved ONNX model bundle.
#[derive(Clone, Debug)]
pub struct OnnxModelResolution {
    /// Path to the ONNX model file.
    pub model_path: PathBuf,
    /// Path to the tokenizer.json file.
    pub tokenizer_path: PathBuf,
    /// Name of the model (for metadata).
    pub model_name: String,
    /// Expected output dimension.
    pub embedding_dim: u16,
    /// Pooling strategy to use.
    pub pooling: PoolingStrategy,
    /// Whether to L2 normalize the output.
    pub normalize: bool,
    /// Whether the model was downloaded (vs. already present).
    pub downloaded: bool,
}

/// Resolved Ollama embedding endpoint.
#[derive(Clone, Debug)]
pub struct OllamaModelResolution {
    /// Base endpoint or full `/api/embed` URL.
    pub endpoint: String,
    /// Ollama model name.
    pub model_name: String,
    /// Expected output dimension.
    pub embedding_dim: u16,
    /// Whether the session should normalize output vectors.
    pub normalize: bool,
    /// Number of texts per request.
    pub batch_size: usize,
}

/// Get the global model cache directory (~/.cache/synrepo/models).
pub fn get_global_cache_dir() -> Result<PathBuf> {
    let base = if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache")
    } else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "Could not determine global cache directory ($HOME not set)"
        )));
    };
    Ok(base.join("synrepo/models"))
}

pub mod resolution;
pub mod session;

pub use resolution::ModelResolver;
pub use session::EmbeddingSession;
