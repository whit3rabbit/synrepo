//! Flat vector index for embedding similarity search.
//!
//! Stores vectors in a flat array and performs brute-force dot product search.
//! Vectors are pre-normalized during index build.

use std::sync::Arc;

use super::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};
use super::model::{EmbeddingSession, ModelResolution};
use super::profile::{VectorPrecision, NORMALIZER_VERSION};

mod persistence;
#[cfg(test)]
mod persistence_tests;
mod quantization;
mod scoring;

use persistence::INDEX_FORMAT_VERSION;

/// A flat vector index for similarity search.
pub struct FlatVecIndex {
    /// Vector dimension.
    pub dim: u16,
    /// Model name (for metadata).
    pub model_name: String,
    /// Format version of the index file.
    pub format_version: u16,
    /// Whether vectors are pre-normalized (enables dot-product similarity).
    pub normalized: bool,
    /// On-disk precision for stored vectors. The in-memory representation
    /// is always `f32`; quantization is a persistence concern.
    pub precision: VectorPrecision,
    /// Format version of the embedder normalization. Bumped when the
    /// semantic of stored vectors changes.
    pub normalizer_version: u16,
    /// The chunk data (IDs and source info).
    pub(crate) chunks: Vec<ChunkMeta>,
    /// Vector data as f32 (dim * n_chunks).
    pub(crate) vectors: Vec<f32>,
    /// Embedding session for on-demand embedding (kept for query-time embedding).
    pub(super) session: Option<Arc<EmbeddingSession>>,
}

impl std::fmt::Debug for FlatVecIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatVecIndex")
            .field("dim", &self.dim)
            .field("model_name", &self.model_name)
            .field("format_version", &self.format_version)
            .field("normalized", &self.normalized)
            .field("precision", &self.precision)
            .field("normalizer_version", &self.normalizer_version)
            .field("chunks_len", &self.chunks.len())
            .field(
                "session",
                &if self.session.is_some() {
                    "Some(...)"
                } else {
                    "None"
                },
            )
            .finish()
    }
}

/// Metadata for a chunk in the index.
#[derive(Clone, Debug)]
pub(crate) struct ChunkMeta {
    pub(crate) id: ChunkId,
    pub(crate) source: EmbeddingChunkSource,
    pub(crate) text: String,
}

impl FlatVecIndex {
    /// Build an index from chunks and a model.
    pub fn build(chunks: Vec<EmbeddingChunk>, model: ModelResolution) -> crate::Result<Self> {
        // Load the model
        let session = super::model::session_cache::shared_from_resolution(&model)?;
        Self::build_with_session_and_progress(
            chunks,
            &model,
            session,
            |_current, _total| {},
            || false,
        )
    }

    /// Build an index using an already initialized session and per-batch hooks.
    ///
    /// The on-disk precision is `VectorPrecision::Float32` by default; callers
    /// that want int8 storage should use
    /// [`Self::build_with_session_and_precision`].
    pub fn build_with_session_and_progress<F, C>(
        chunks: Vec<EmbeddingChunk>,
        model: &ModelResolution,
        session: Arc<EmbeddingSession>,
        on_batch: F,
        should_stop: C,
    ) -> crate::Result<Self>
    where
        F: FnMut(usize, usize),
        C: FnMut() -> bool,
    {
        Self::build_with_session_and_precision(
            chunks,
            model,
            session,
            VectorPrecision::Float32,
            on_batch,
            should_stop,
        )
    }

    /// Iterate over (ChunkId, text, vector_slice) tuples for each stored chunk.
    pub(crate) fn iter_chunks_with_vectors(
        &self,
    ) -> impl Iterator<Item = (&ChunkId, &str, &[f32])> {
        let dim = self.dim as usize;
        self.chunks.iter().enumerate().map(move |(i, c)| {
            let start = i * dim;
            let end = start + dim;
            (&c.id, c.text.as_str(), &self.vectors[start..end])
        })
    }

    /// Like [`Self::build_with_session_and_progress`] but writes vectors in
    /// the declared precision. Memory always holds `f32`; quantization is
    /// applied at persistence time.
    pub fn build_with_session_and_precision<F, C>(
        chunks: Vec<EmbeddingChunk>,
        model: &ModelResolution,
        session: Arc<EmbeddingSession>,
        precision: VectorPrecision,
        on_batch: F,
        should_stop: C,
    ) -> crate::Result<Self>
    where
        F: FnMut(usize, usize),
        C: FnMut() -> bool,
    {
        Self::build_with_session_precision_and_reuse(
            chunks,
            model,
            Some(session),
            precision,
            None,
            on_batch,
            should_stop,
        )
    }

    /// Build or refresh an index with optional vector reuse from an existing index.
    pub fn build_with_session_precision_and_reuse<F, C>(
        chunks: Vec<EmbeddingChunk>,
        model: &ModelResolution,
        session: Option<Arc<EmbeddingSession>>,
        precision: VectorPrecision,
        reuse: Option<&super::reuse::VectorReuse>,
        mut on_batch: F,
        mut should_stop: C,
    ) -> crate::Result<Self>
    where
        F: FnMut(usize, usize),
        C: FnMut() -> bool,
    {
        let dim = model.embedding_dim() as usize;
        let total = chunks.len();

        let mut flat_vectors = vec![0.0f32; total * dim];
        let mut miss_indices = Vec::new();

        if let Some(reuse) = reuse {
            for (idx, maybe_vec) in reuse.chunk_vectors.iter().enumerate() {
                if let Some(vec) = maybe_vec {
                    let start = idx * dim;
                    flat_vectors[start..start + dim].copy_from_slice(vec);
                } else {
                    miss_indices.push(idx);
                }
            }
        } else {
            miss_indices.extend(0..total);
        }

        let reused_count = total - miss_indices.len();
        if reused_count > 0 {
            on_batch(reused_count, total);
        }

        if !miss_indices.is_empty() {
            let session = session.as_ref().ok_or_else(|| {
                crate::Error::Other(anyhow::anyhow!(
                    "embedding session required for miss chunks"
                ))
            })?;
            let batch_size = model.build_batch_size();
            let mut current = reused_count;
            for chunk_slice in miss_indices.chunks(batch_size) {
                if should_stop() {
                    return Err(crate::Error::Other(anyhow::anyhow!(
                        "embedding build cancelled"
                    )));
                }
                let texts: Vec<String> = chunk_slice
                    .iter()
                    .map(|&idx| chunks[idx].text.clone())
                    .collect();
                let vectors = session.embed(&texts)?;
                if vectors.len() != chunk_slice.len() {
                    return Err(crate::Error::Other(anyhow::anyhow!(
                        "embedding provider returned {} vectors for {} chunks",
                        vectors.len(),
                        chunk_slice.len()
                    )));
                }
                for (&target_idx, vector) in chunk_slice.iter().zip(vectors) {
                    if vector.len() != dim {
                        return Err(crate::Error::Other(anyhow::anyhow!(
                            "embedding vector has dimension {}, expected {}",
                            vector.len(),
                            dim
                        )));
                    }
                    let start = target_idx * dim;
                    flat_vectors[start..start + dim].copy_from_slice(&vector);
                }
                current += chunk_slice.len();
                on_batch(current, total);
            }
        }

        // Store chunk metadata
        let chunk_metas: Vec<ChunkMeta> = chunks
            .into_iter()
            .map(|c| ChunkMeta {
                id: c.id,
                source: c.source,
                text: c.text,
            })
            .collect();

        Ok(Self {
            dim: model.embedding_dim(),
            model_name: model.model_name().to_string(),
            format_version: INDEX_FORMAT_VERSION,
            normalized: model.normalize(),
            precision,
            normalizer_version: NORMALIZER_VERSION,
            chunks: chunk_metas,
            vectors: flat_vectors,
            session,
        })
    }

    /// Embed a query string and return the vector.
    ///
    /// Used for query-time embedding during semantic triage. Applies the
    /// model's configured query prefix (e.g. the instruction prefix required
    /// by `snowflake-arctic-embed-xs`) when one is set.
    pub fn embed_text(&self, text: &str) -> crate::Result<Vec<f32>> {
        let session = self.session.as_ref().ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!(
                "Embedding session not available. Use load_with_resolution() to restore a session."
            ))
        })?;
        session.embed_query(text)
    }

    /// Get the symbol node ID from a chunk ID if it's a symbol chunk.
    pub fn chunk_to_symbol_id(&self, chunk_id: &ChunkId) -> Option<crate::core::ids::SymbolNodeId> {
        self.chunks
            .iter()
            .find(|c| c.id == *chunk_id)
            .and_then(|c| {
                if let EmbeddingChunkSource::Symbol { id, .. } = &c.source {
                    Some(*id)
                } else {
                    None
                }
            })
    }

    /// Get the source metadata for a chunk.
    pub fn chunk_source(&self, chunk_id: &ChunkId) -> Option<EmbeddingChunkSource> {
        self.chunks
            .iter()
            .find(|c| c.id == *chunk_id)
            .map(|c| c.source.clone())
    }

    /// Get the stored text for a chunk.
    pub fn chunk_text(&self, chunk_id: &ChunkId) -> Option<&str> {
        self.chunks
            .iter()
            .find(|c| c.id == *chunk_id)
            .map(|c| c.text.as_str())
    }

    /// Get the number of chunks in the index.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_text_fails_when_session_missing() {
        let index = FlatVecIndex {
            dim: 4,
            model_name: "test".into(),
            format_version: INDEX_FORMAT_VERSION,
            normalized: true,
            precision: VectorPrecision::Float32,
            normalizer_version: NORMALIZER_VERSION,
            chunks: vec![],
            vectors: vec![],
            session: None,
        };
        let err = index.embed_text("any query").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("load_with_resolution"),
            "expected error explaining session requirement, got: {msg}"
        );
    }
}
