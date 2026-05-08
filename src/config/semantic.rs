use serde::{Deserialize, Serialize};

/// Local embedding backend used by semantic triage and hybrid search.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticEmbeddingProvider {
    /// Built-in ONNX Runtime model execution.
    #[default]
    Onnx,
    /// Local Ollama `/api/embed` endpoint.
    Ollama,
}

/// Whether the embedding provider was written in config or supplied by the
/// legacy default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SemanticProviderSource {
    /// The config file explicitly named `semantic_embedding_provider`.
    Explicit,
    /// The field was omitted, so the historical ONNX default was used.
    #[default]
    Defaulted,
}

impl SemanticEmbeddingProvider {
    /// Stable config and compatibility label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Onnx => "onnx",
            Self::Ollama => "ollama",
        }
    }
}

impl SemanticProviderSource {
    /// Stable status/API label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Defaulted => "defaulted",
        }
    }
}

pub(crate) fn default_semantic_embedding_provider() -> SemanticEmbeddingProvider {
    SemanticEmbeddingProvider::Onnx
}

pub(crate) fn default_semantic_model() -> String {
    "all-MiniLM-L6-v2".to_string()
}

pub(crate) fn default_embedding_dim() -> u16 {
    384
}

pub(crate) fn default_semantic_similarity_threshold() -> f64 {
    0.6
}

pub(crate) fn default_semantic_ollama_endpoint() -> String {
    "http://localhost:11434".to_string()
}

pub(crate) fn default_semantic_embedding_batch_size() -> usize {
    128
}
