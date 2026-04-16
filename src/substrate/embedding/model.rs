//! Embedding model resolution and ONNX inference.
//!
//! Handles model resolution (built-in, Hugging Face, local path),
//! downloading, and inference using the `ort` and `tokenizers` crates.

use parking_lot::Mutex;
use std::path::{Path, PathBuf};

use crate::Result;

#[cfg(feature = "semantic-triage")]
use ndarray::Array2;
#[cfg(feature = "semantic-triage")]
use ort::value::Value;

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

/// Model resolver for embedding models.
pub struct ModelResolver;

impl ModelResolver {
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

        // Check if it's a Hugging Face model ID (contains `/`)
        if model_id.contains('/') {
            return self.resolve_huggingface(model_id, &model_cache_dir, declared_dim);
        }

        // Check if it's an absolute path to a .onnx file
        let onnx_path = PathBuf::from(model_id);
        if onnx_path.is_absolute() && model_id.ends_with(".onnx") {
            let tokenizer_path = onnx_path.with_file_name("tokenizer.json");
            if !tokenizer_path.exists() {
                return Err(crate::Error::Config(format!(
                    "Local model '{}' found but accompanying 'tokenizer.json' is missing in the same directory",
                    model_id
                )));
            }
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

        let bytes = response.bytes().map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("Failed to read download response: {}", e))
        })?;

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

/// ONNX inference session wrapper with tokenizer support.
#[derive(Debug)]
pub struct EmbeddingSession {
    dim: u16,
    pooling: PoolingStrategy,
    normalize: bool,
    tokenizer: tokenizers::Tokenizer,
    session: Mutex<ort::session::Session>,
}

impl EmbeddingSession {
    /// Create a new session from a model resolution.
    pub fn new_from_resolution(res: &ModelResolution) -> Result<Self> {
        let tokenizer = tokenizers::Tokenizer::from_file(&res.tokenizer_path)
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Failed to load tokenizer: {}", e)))?;

        let session = ort::session::Session::builder()
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Failed to create session: {}", e)))?
            .commit_from_file(&res.model_path)
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Failed to load model: {}", e)))?;

        Ok(Self {
            dim: res.embedding_dim,
            pooling: res.pooling,
            normalize: res.normalize,
            tokenizer,
            session: Mutex::new(session),
        })
    }

    /// Run inference on a batch of texts.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            // Tokenize
            let encoding = self
                .tokenizer
                .encode(text.as_str(), true)
                .map_err(|e| crate::Error::Other(anyhow::anyhow!("Tokenization failed: {}", e)))?;

            let input_ids = encoding.get_ids();
            let attention_mask = encoding.get_attention_mask();
            let type_ids = encoding.get_type_ids();
            let seq_len = input_ids.len();

            // Convert to ndarray for ort
            let input_ids_arr =
                Array2::from_shape_vec((1, seq_len), input_ids.iter().map(|&x| x as i64).collect())
                    .map_err(|e| {
                        crate::Error::Other(anyhow::anyhow!("Array shape error: {}", e))
                    })?;
            let attention_mask_arr = Array2::from_shape_vec(
                (1, seq_len),
                attention_mask.iter().map(|&x| x as i64).collect(),
            )
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Array shape error: {}", e)))?;
            let type_ids_arr =
                Array2::from_shape_vec((1, seq_len), type_ids.iter().map(|&x| x as i64).collect())
                    .map_err(|e| {
                        crate::Error::Other(anyhow::anyhow!("Array shape error: {}", e))
                    })?;

            // Run inference
            let inputs = ort::inputs![
                "input_ids" => Value::from_array(input_ids_arr).map_err(|e| crate::Error::Other(e.into()))?,
                "attention_mask" => Value::from_array(attention_mask_arr).map_err(|e| crate::Error::Other(e.into()))?,
                "token_type_ids" => Value::from_array(type_ids_arr).map_err(|e| crate::Error::Other(e.into()))?,
            ];

            let mut session = self.session.lock();
            let outputs = session
                .run(inputs)
                .map_err(|e| crate::Error::Other(anyhow::anyhow!("Inference failed: {}", e)))?;

            // Extract last_hidden_state
            let last_hidden_state = &outputs["last_hidden_state"];

            // Pooling
            let pooled = match self.pooling {
                PoolingStrategy::Mean => mean_pooling(last_hidden_state, attention_mask),
                PoolingStrategy::Cls => cls_pooling(last_hidden_state, self.dim as usize),
            };

            // Normalize
            let final_vec = if self.normalize {
                normalize(pooled)
            } else {
                pooled
            };

            results.push(final_vec);
        }

        Ok(results)
    }

    /// Get the embedding dimension.
    pub fn embedding_dim(&self) -> u16 {
        self.dim
    }
}

fn mean_pooling(last_hidden_state: &Value, attention_mask: &[u32]) -> Vec<f32> {
    let (shape, data) = last_hidden_state.try_extract_tensor::<f32>().unwrap();
    let seq_len = shape[1] as usize;
    let dim = shape[2] as usize;

    let mut sum = vec![0.0f32; dim];
    let mut count = 0.0f32;

    for i in 0..seq_len {
        if i < attention_mask.len() && attention_mask[i] == 1 {
            count += 1.0;
            for j in 0..dim {
                sum[j] += data[i * dim + j];
            }
        }
    }

    if count > 0.0 {
        for val in sum.iter_mut() {
            *val /= count;
        }
    }

    sum
}

fn cls_pooling(last_hidden_state: &Value, dim: usize) -> Vec<f32> {
    let (_shape, data) = last_hidden_state.try_extract_tensor::<f32>().unwrap();
    let mut cls = vec![0.0f32; dim];
    // CLS is the first vector in the sequence (index 0)
    for i in 0..dim {
        cls[i] = data[i];
    }
    cls
}

fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for val in v.iter_mut() {
            *val /= norm;
        }
    }
    v
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
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
}
