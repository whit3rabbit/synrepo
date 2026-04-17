//! Model resolution: built-in registry, HuggingFace download, local path lookup.

use std::path::{Path, PathBuf};

use crate::core::path_safety::{has_windows_prefix_component, looks_like_unc};
use crate::Result;

use super::{get_global_cache_dir, ModelResolution, PoolingStrategy};

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

    /// Resolve a model identifier to a local model bundle.
    pub fn resolve(
        &self,
        model_id: &str,
        _repo_synrepo_dir: &Path, // Not used anymore as we use global cache
        declared_dim: u16,
    ) -> Result<ModelResolution> {
        // Resolve to global cache
        let cache_base = get_global_cache_dir()?;
        let model_cache_dir = cache_base.join(model_id.replace('/', "--"));
        std::fs::create_dir_all(&model_cache_dir)?;

        // Check built-in registry
        for spec in BUILTIN_MODELS {
            if spec.model_id == model_id {
                return self.resolve_spec(spec, &model_cache_dir, declared_dim);
            }
        }

        // Check if it's an absolute path to a .onnx file (check before HF,
        // since absolute paths contain `/` which would match the HF heuristic).
        let onnx_path = PathBuf::from(model_id);
        if onnx_path.is_absolute() && model_id.ends_with(".onnx") {
            // Refuse UNC / verbatim / device prefixes before touching the
            // filesystem. On Windows these trigger remote SMB auth (leaking
            // NTLM hashes) and would otherwise hand an attacker-chosen ORT
            // graph to the embedding runtime. The string-level `looks_like_unc`
            // check belt-and-braces the same rejection on every platform —
            // on Unix `Path::is_absolute` returns false for `\\host\share`,
            // but we still want to refuse it consistently.
            if has_windows_prefix_component(&onnx_path) || looks_like_unc(model_id) {
                return Err(crate::Error::Config(format!(
                    "semantic_model '{}' uses a UNC or device path; refusing to load",
                    model_id
                )));
            }
            let tokenizer_path = onnx_path.with_file_name("tokenizer.json");
            if !tokenizer_path.exists() {
                return Err(crate::Error::Config(format!(
                    "Local model '{}' found but accompanying 'tokenizer.json' is missing in the same directory",
                    model_id
                )));
            }
            tracing::warn!(
                model = model_id,
                pooling = "mean",
                normalize = true,
                "Using default pooling=mean and normalize=true for custom local model. \
                 If this model expects CLS pooling, set semantic_pooling in config."
            );
            return Ok(ModelResolution {
                model_path: onnx_path,
                tokenizer_path,
                model_name: model_id.to_string(),
                embedding_dim: declared_dim,
                pooling: PoolingStrategy::Mean, // Default for custom models
                normalize: true,                // Default for custom models
                downloaded: false,
            });
        }

        // Check if it's a Hugging Face model ID (contains `/` but not an absolute path).
        if model_id.contains('/') {
            return self.resolve_huggingface(model_id, &model_cache_dir, declared_dim);
        }

        Err(crate::Error::Config(format!(
            "Invalid model identifier '{}'. Expected a built-in name ({}), a Hugging Face model ID (e.g. 'intfloat/e5-base-v2'), or an absolute path to a .onnx file",
            model_id,
            BUILTIN_MODELS.iter().map(|s| s.model_id).collect::<Vec<_>>().join(", ")
        )))
    }

    fn resolve_spec(
        &self,
        spec: &EmbeddingModelSpec,
        cache_dir: &Path,
        declared_dim: u16,
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
            self.download_file(spec.onnx_url, &model_path)?;
            downloaded = true;
        }
        if !tokenizer_path.exists() {
            self.download_file(spec.tokenizer_url, &tokenizer_path)?;
            downloaded = true;
        }

        Ok(ModelResolution {
            model_path,
            tokenizer_path,
            model_name: spec.model_id.to_string(),
            embedding_dim: spec.expected_dim,
            pooling: spec.pooling,
            normalize: spec.normalize,
            downloaded,
        })
    }

    fn resolve_huggingface(
        &self,
        model_id: &str,
        cache_dir: &Path,
        declared_dim: u16,
    ) -> Result<ModelResolution> {
        let onnx_url = format!(
            "https://huggingface.co/{}/resolve/main/model.onnx",
            model_id
        );
        let tokenizer_url = format!(
            "https://huggingface.co/{}/resolve/main/tokenizer.json",
            model_id
        );

        let model_path = cache_dir.join("model.onnx");
        let tokenizer_path = cache_dir.join("tokenizer.json");

        let mut downloaded = false;
        if !model_path.exists() {
            self.download_file(&onnx_url, &model_path)?;
            downloaded = true;
        }
        if !tokenizer_path.exists() {
            self.download_file(&tokenizer_url, &tokenizer_path)?;
            downloaded = true;
        }

        tracing::warn!(
            model = model_id,
            pooling = "mean",
            normalize = true,
            "Assuming mean pooling and L2 normalization for Hugging Face model. \
             sentence-transformers models typically use mean pooling, but verify for non-st models."
        );

        Ok(ModelResolution {
            model_path,
            tokenizer_path,
            model_name: model_id.to_string(),
            embedding_dim: declared_dim,
            pooling: PoolingStrategy::Mean, // Default guess for HF models
            normalize: true,
            downloaded,
        })
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
        // This might fail if $HOME is not set or if we can't write to the cache.
        // We accept resolution success, or a clear "no HOME" error, or an IO error.
        let result = resolver.resolve("all-MiniLM-L6-v2", Path::new("."), 384);
        match result {
            Ok(_) => {}
            Err(crate::Error::Io(_)) => {}
            Err(e) if e.to_string().contains("HOME") => {}
            Err(e) if e.to_string().contains("Download failed") => {}
            Err(e) if e.to_string().contains("download lock") => {}
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn resolve_bad_onnx_path_returns_error() {
        let resolver = ModelResolver::new();
        let result = resolver.resolve("/nonexistent/path/model.onnx", Path::new("."), 384);
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("tokenizer") || err.to_string().contains("missing"),
            "Expected clear error about missing tokenizer, got: {err}"
        );
    }

    #[test]
    fn resolve_onnx_without_tokenizer_returns_config_error() {
        let dir = tempfile::tempdir().unwrap();
        let onnx_path = dir.path().join("model.onnx");
        std::fs::write(&onnx_path, b"fake onnx").unwrap();
        // tokenizer.json deliberately not created
        let resolver = ModelResolver::new();
        let result = resolver.resolve(onnx_path.to_str().unwrap(), Path::new("."), 384);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("tokenizer"),
            "Expected tokenizer mention in error, got: {err}"
        );
    }

    #[test]
    fn resolve_builtin_wrong_dim_returns_config_error() {
        let resolver = ModelResolver::new();
        let result = resolver.resolve("all-MiniLM-L6-v2", Path::new("."), 999);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("384") || err.to_string().contains("embedding_dim"),
            "Expected dimension mismatch error, got: {err}"
        );
    }

    #[test]
    fn resolve_rejects_unc_style_slash_path() {
        // `//attacker/share/m.onnx` is absolute on Unix and on Windows, so
        // it hits the local-ONNX branch; the UNC check must reject it before
        // any filesystem probe that could leak NTLM or load a foreign ORT
        // graph.
        let resolver = ModelResolver::new();
        let err = resolver
            .resolve("//attacker/share/m.onnx", Path::new("."), 384)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("UNC") || msg.contains("device"),
            "Expected UNC/device rejection, got: {msg}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolve_rejects_backslash_unc_path() {
        let resolver = ModelResolver::new();
        let err = resolver
            .resolve(r"\\attacker\share\m.onnx", Path::new("."), 384)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("UNC") || msg.contains("device"),
            "Expected UNC/device rejection, got: {msg}"
        );
    }
}
