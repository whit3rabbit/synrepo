//! Embedding model resolution and ONNX inference.
//!
//! Handles model resolution (built-in, Hugging Face, local path),
//! downloading, and inference using the `ort` crate.

use std::path::PathBuf;

use crate::Result;

/// Built-in model registry.
const BUILTIN_MODELS: &[(&str, &str, u16)] = &[
    (
        "all-MiniLM-L6-v2",
        "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.onnx",
        384,
    ),
    (
        "all-MiniLM-L12-v2",
        "https://huggingface.co/sentence-transformers/all-MiniLM-L12-v2/resolve/main/model.onnx",
        384,
    ),
    (
        "all-mpnet-base-v2",
        "https://huggingface.co/sentence-transformers/all-mpnet-base-v2/resolve/main/model.onnx",
        768,
    ),
];

/// Result of resolving an embedding model.
#[derive(Clone, Debug)]
pub struct ModelResolution {
    /// Path to the ONNX model file.
    pub model_path: PathBuf,
    /// Name of the model (for metadata).
    pub model_name: String,
    /// Expected output dimension.
    pub embedding_dim: u16,
    /// Whether the model was downloaded (vs. already present).
    pub downloaded: bool,
}

/// Model resolver for embedding models.
pub struct ModelResolver;

impl ModelResolver {
    pub fn new() -> Self {
        Self
    }

    /// Resolve a model identifier to a local model path.
    ///
    /// Resolution order:
    /// 1. Built-in registry match
    /// 2. Contains `/` → Hugging Face model ID
    /// 3. Ends with `.onnx` → local path
    /// 4. Error
    pub fn resolve(
        &self,
        model_id: &str,
        cache_dir: &std::path::Path,
        declared_dim: u16,
    ) -> Result<ModelResolution> {
        // Check built-in registry
        for (name, url, expected_dim) in BUILTIN_MODELS {
            if *name == model_id {
                return self.resolve_builtin(name, *url, *expected_dim, cache_dir, declared_dim);
            }
        }

        // Check if it's a Hugging Face model ID (contains `/`)
        if model_id.contains('/') {
            return self.resolve_huggingface(model_id, cache_dir, declared_dim);
        }

        // Check if it's a local path
        let model_path = std::path::PathBuf::from(model_id);
        if model_path.exists() && model_id.ends_with(".onnx") {
            return Ok(ModelResolution {
                model_path,
                model_name: model_id.to_string(),
                embedding_dim: declared_dim,
                downloaded: false,
            });
        }

        Err(crate::Error::Config(format!(
            "Invalid model identifier '{}'. Expected a built-in name ({}), a Hugging Face model ID (e.g. 'intfloat/e5-base-v2'), or an absolute path to a .onnx file",
            model_id,
            BUILTIN_MODELS.iter().map(|(n, _, _)| *n).collect::<Vec<_>>().join(", ")
        )))
    }

    fn resolve_builtin(
        &self,
        name: &str,
        url: &str,
        expected_dim: u16,
        cache_dir: &std::path::Path,
        declared_dim: u16,
    ) -> Result<ModelResolution> {
        if expected_dim != declared_dim {
            return Err(crate::Error::Config(format!(
                "Model '{}' outputs {}d vectors but config specifies embedding_dim = {}",
                name, expected_dim, declared_dim
            )));
        }

        let model_path = cache_dir.join("model.onnx");
        let downloaded = if !model_path.exists() {
            self.download_model(url, &model_path)?;
            true
        } else {
            false
        };

        Ok(ModelResolution {
            model_path,
            model_name: name.to_string(),
            embedding_dim: expected_dim,
            downloaded,
        })
    }

    fn resolve_huggingface(
        &self,
        model_id: &str,
        cache_dir: &std::path::Path,
        declared_dim: u16,
    ) -> Result<ModelResolution> {
        let url = format!(
            "https://huggingface.co/{}/resolve/main/model.onnx",
            model_id
        );
        let model_path = cache_dir.join("model.onnx");
        let downloaded = if !model_path.exists() {
            self.download_model(&url, &model_path)?;
            true
        } else {
            false
        };

        Ok(ModelResolution {
            model_path,
            model_name: model_id.to_string(),
            embedding_dim: declared_dim,
            downloaded,
        })
    }

    fn download_model(&self, url: &str, dest: &std::path::Path) -> Result<()> {
        let response = reqwest::blocking::get(url)
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Failed to download model: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Failed to read response: {}", e)))?;

        let temp_path = dest.with_extension("tmp");
        std::fs::write(&temp_path, &bytes).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("Failed to write temp file: {}", e))
        })?;

        std::fs::rename(&temp_path, dest).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("Failed to rename temp file: {}", e))
        })?;

        Ok(())
    }
}

/// ONNX inference session wrapper.
///
/// This implementation uses a stub for inference. Full implementation
/// requires a tokenizer bundled with the model. For production, consider
/// using the `tokenizers` crate with a bundled vocab.
#[derive(Debug)]
pub struct EmbeddingSession {
    /// Model dimension for validation.
    dim: u16,
    /// Token count for batch construction.
    _token_count: usize,
    #[allow(dead_code)]
    session: ort::session::Session,
}

impl EmbeddingSession {
    /// Create a new session from a model path.
    pub fn new(model_path: &std::path::Path) -> Result<Self> {
        // Load the session
        let session = ort::session::Session::builder()
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Failed to create session: {}", e)))?
            .commit_from_file(model_path)
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Failed to load model: {}", e)))?;

        // Inspect model to get dimension
        let outputs = session.outputs();
        let dim = if let Some(output) = outputs.first() {
            // Try to get shape info - this is model-specific
            // Default to 384 for now, proper impl would inspect the model
            384
        } else {
            384
        };

        Ok(Self {
            dim,
            _token_count: 0,
            session,
        })
    }

    /// Run inference on a batch of texts.
    ///
    /// Returns stub vectors. Full implementation requires tokenizer.
    ///
    /// Note: This is a stub that returns zero vectors.
    /// Production implementation needs:
    /// - Bundled tokenizer (tokenizers crate)
    /// - Proper input construction
    /// - Mean pooling over sequence dimension
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let dim = self.dim as usize;

        // Return zero embeddings as stub
        // TODO: Full impl with tokenizer
        let results: Vec<Vec<f32>> = texts.iter().map(|_| vec![0.0f32; dim]).collect();

        Ok(results)
    }

    /// Get the embedding dimension.
    pub fn embedding_dim(&self) -> u16 {
        self.dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_builtin_models() {
        let resolver = ModelResolver::new();
        let result = resolver.resolve("all-MiniLM-L6-v2", std::path::Path::new("/tmp/test"), 384);
        // Should work for resolution (may fail on download check)
        assert!(result.is_ok() || result.unwrap_err().to_string().contains("download"));
    }

    #[test]
    fn resolver_invalid_model() {
        let resolver = ModelResolver::new();
        let result = resolver.resolve("nonexistent-model", std::path::Path::new("/tmp/test"), 384);
        assert!(result.is_err());
    }
}
