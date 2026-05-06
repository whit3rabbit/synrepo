//! Model resolution: built-in registry, HuggingFace download, local path lookup.

use std::path::Path;

use crate::config::{Config, SemanticEmbeddingProvider};
use crate::Result;

use super::{
    get_global_cache_dir, ModelResolution, OllamaModelResolution, OnnxModelResolution,
    PoolingStrategy,
};

/// Built-in model registry with explicit specs.
const BUILTIN_MODELS: &[EmbeddingModelSpec] = &[
    EmbeddingModelSpec {
        model_id: "all-MiniLM-L6-v2",
        onnx_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx",
        tokenizer_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json",
        expected_dim: 384,
        pooling: PoolingStrategy::Mean,
        normalize: true,
    },
    EmbeddingModelSpec {
        model_id: "all-MiniLM-L12-v2",
        onnx_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L12-v2/resolve/main/onnx/model.onnx",
        tokenizer_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L12-v2/resolve/main/tokenizer.json",
        expected_dim: 384,
        pooling: PoolingStrategy::Mean,
        normalize: true,
    },
    EmbeddingModelSpec {
        model_id: "all-mpnet-base-v2",
        onnx_url: "https://huggingface.co/sentence-transformers/all-mpnet-base-v2/resolve/main/model.onnx",
        tokenizer_url: "https://huggingface.co/sentence-transformers/all-mpnet-base-v2/resolve/main/tokenizer.json",
        expected_dim: 768,
        pooling: PoolingStrategy::Mean,
        normalize: true,
    },
];

/// Specification for a supported embedding model.
#[derive(Clone, Debug)]
struct EmbeddingModelSpec {
    model_id: &'static str,
    onnx_url: &'static str,
    tokenizer_url: &'static str,
    expected_dim: u16,
    pooling: PoolingStrategy,
    normalize: bool,
}

/// Model resolver for embedding models.
#[derive(Default)]
pub struct ModelResolver;

impl ModelResolver {
    /// Create a new model resolver.
    pub fn new() -> Self {
        Self
    }

    /// Resolve a configured model to a local model bundle or local endpoint.
    pub fn resolve(&self, config: &Config, _repo_synrepo_dir: &Path) -> Result<ModelResolution> {
        self.resolve_config(config, true)
    }

    /// Resolve a model only if its artifacts are already present locally.
    ///
    /// Query-time surfaces use this so semantic routing/search availability
    /// never triggers a network download on an agent hook or MCP request.
    pub fn resolve_existing(
        &self,
        config: &Config,
        _repo_synrepo_dir: &Path,
    ) -> Result<ModelResolution> {
        self.resolve_config(config, false)
    }

    fn resolve_config(&self, config: &Config, allow_download: bool) -> Result<ModelResolution> {
        match config.semantic_embedding_provider {
            SemanticEmbeddingProvider::Onnx => {
                self.resolve_inner(&config.semantic_model, config.embedding_dim, allow_download)
            }
            SemanticEmbeddingProvider::Ollama => self.resolve_ollama(config),
        }
    }

    fn resolve_ollama(&self, config: &Config) -> Result<ModelResolution> {
        if config.semantic_embedding_batch_size == 0 {
            return Err(crate::Error::Config(
                "semantic_embedding_batch_size must be greater than 0".to_string(),
            ));
        }
        if config.semantic_ollama_endpoint.trim().is_empty() {
            return Err(crate::Error::Config(
                "semantic_ollama_endpoint must not be empty".to_string(),
            ));
        }
        Ok(ModelResolution::Ollama(OllamaModelResolution {
            endpoint: config.semantic_ollama_endpoint.clone(),
            model_name: config.semantic_model.clone(),
            embedding_dim: config.embedding_dim,
            normalize: true,
            batch_size: config.semantic_embedding_batch_size,
        }))
    }

    fn resolve_inner(
        &self,
        model_id: &str,
        declared_dim: u16,
        allow_download: bool,
    ) -> Result<ModelResolution> {
        // Resolve to global cache
        let cache_base = get_global_cache_dir()?;
        let model_cache_dir = cache_base.join(model_id.replace('/', "--"));
        if allow_download {
            std::fs::create_dir_all(&model_cache_dir)?;
        }

        // Check built-in registry
        for spec in BUILTIN_MODELS {
            if spec.model_id == model_id {
                return self.resolve_spec(spec, &model_cache_dir, declared_dim, allow_download);
            }
        }

        Err(crate::Error::Config(format!(
            "Invalid model identifier '{}'. Expected a built-in name ({})",
            model_id,
            BUILTIN_MODELS
                .iter()
                .map(|s| s.model_id)
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    fn resolve_spec(
        &self,
        spec: &EmbeddingModelSpec,
        cache_dir: &Path,
        declared_dim: u16,
        allow_download: bool,
    ) -> Result<ModelResolution> {
        if spec.expected_dim != declared_dim {
            return Err(crate::Error::Config(format!(
                "Model '{}' outputs {}d vectors but config specifies embedding_dim = {}",
                spec.model_id, spec.expected_dim, declared_dim
            )));
        }

        let model_path = cache_dir.join("model.onnx");
        let tokenizer_path = cache_dir.join("tokenizer.json");

        let mut downloaded = false;
        if !model_path.exists() {
            if !allow_download {
                return Err(crate::Error::Other(anyhow::anyhow!(
                    "model artifact is not cached at {}",
                    model_path.display()
                )));
            }
            self.download_file(spec.onnx_url, &model_path)?;
            downloaded = true;
        }
        if !tokenizer_path.exists() {
            if !allow_download {
                return Err(crate::Error::Other(anyhow::anyhow!(
                    "tokenizer artifact is not cached at {}",
                    tokenizer_path.display()
                )));
            }
            self.download_file(spec.tokenizer_url, &tokenizer_path)?;
            downloaded = true;
        }

        Ok(ModelResolution::Onnx(OnnxModelResolution {
            model_path,
            tokenizer_path,
            model_name: spec.model_id.to_string(),
            embedding_dim: spec.expected_dim,
            pooling: spec.pooling,
            normalize: spec.normalize,
            downloaded,
        }))
    }

    fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        // Acquire advisory lock to prevent concurrent download corruption.
        let lock_path = dest.with_extension("download.lock");
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| {
                crate::Error::Other(anyhow::anyhow!(
                    "Failed to open download lock at {}: {}",
                    lock_path.display(),
                    e
                ))
            })?;

        fs2::FileExt::lock_exclusive(&lock_file).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!(
                "Failed to acquire download lock at {}: {}",
                lock_path.display(),
                e
            ))
        })?;

        // Double-check: another process may have completed the download while we waited.
        if dest.exists() {
            return Ok(());
        }

        let response = reqwest::blocking::get(url).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("Failed to download model artifact: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "Download failed for {} with status: {}",
                url,
                response.status()
            )));
        }

        let temp_path = dest.with_extension("tmp");
        let mut file = std::fs::File::create(&temp_path).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("Failed to create temp file: {}", e))
        })?;

        let mut response = response;
        response.copy_to(&mut file).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("Failed to read download response: {}", e))
        })?;

        std::fs::rename(&temp_path, dest).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("Failed to rename temp file: {}", e))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_builtin_models() {
        let resolver = ModelResolver::new();
        let mut config = crate::config::Config::default();
        config.semantic_model = "all-MiniLM-L6-v2".to_string();
        config.embedding_dim = 384;
        // This might fail if $HOME is not set or if we can't write to the cache.
        // We accept resolution success, or a clear "no HOME" error, or an IO error.
        let result = resolver.resolve(&config, Path::new("."));
        match result {
            Ok(_) => {}
            Err(crate::Error::Io(_)) => {}
            Err(e) if e.to_string().contains("HOME") => {}
            Err(e) if e.to_string().contains("Download failed") => {}
            Err(e) if e.to_string().contains("download lock") => {}
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
}
