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
pub struct ModelResolution {
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
