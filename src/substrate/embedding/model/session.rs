//! ONNX inference session wrapper with tokenizer support.

use parking_lot::Mutex;

use crate::Result;

use super::{ModelResolution, PoolingStrategy};

#[cfg(feature = "semantic-triage")]
use ndarray::Array2;
#[cfg(feature = "semantic-triage")]
use ort::value::Value;

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
                PoolingStrategy::Mean => mean_pooling(last_hidden_state, attention_mask)?,
                PoolingStrategy::Cls => cls_pooling(last_hidden_state, self.dim as usize)?,
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

fn mean_pooling(last_hidden_state: &Value, attention_mask: &[u32]) -> Result<Vec<f32>> {
    let (shape, data) = last_hidden_state.try_extract_tensor::<f32>().map_err(|e| {
        crate::Error::Other(anyhow::anyhow!(
            "Tensor extraction failed in mean_pooling: {}",
            e
        ))
    })?;

    if shape.len() != 3 {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "Expected 3D tensor [1, seq_len, dim] in mean_pooling, got {}D",
            shape.len()
        )));
    }

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

    Ok(sum)
}

fn cls_pooling(last_hidden_state: &Value, dim: usize) -> Result<Vec<f32>> {
    let (shape, data) = last_hidden_state.try_extract_tensor::<f32>().map_err(|e| {
        crate::Error::Other(anyhow::anyhow!(
            "Tensor extraction failed in cls_pooling: {}",
            e
        ))
    })?;

    if shape.len() != 3 {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "Expected 3D tensor [1, seq_len, dim] in cls_pooling, got {}D",
            shape.len()
        )));
    }

    let actual_dim = shape[2] as usize;
    if actual_dim < dim {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "CLS pooling: tensor dim {} is less than expected {}",
            actual_dim,
            dim
        )));
    }

    let mut cls = vec![0.0f32; dim];
    cls[..dim].copy_from_slice(&data[..dim]);
    Ok(cls)
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

    /// Verify that `normalize` does not divide by zero for a zero vector.
    #[test]
    fn normalize_zero_vector_returns_zero() {
        let v = vec![0.0f32; 4];
        let result = normalize(v);
        assert!(result.iter().all(|&x| x == 0.0));
    }

    /// Verify L2 normalization produces a unit vector.
    #[test]
    fn normalize_produces_unit_vector() {
        let v = vec![3.0f32, 4.0];
        let result = normalize(v);
        let norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6, "expected unit norm, got {norm}");
    }
}
