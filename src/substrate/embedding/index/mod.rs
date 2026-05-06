//! Flat vector index for embedding similarity search.
//!
//! Stores vectors in a flat array and performs brute-force dot product search.
//! Vectors are pre-normalized during index build.

use super::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};
use super::model::{EmbeddingSession, ModelResolution};

mod persistence;
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
    /// The chunk data (IDs and source info).
    pub(super) chunks: Vec<ChunkMeta>,
    /// Vector data as f32 (dim * n_chunks).
    pub(super) vectors: Vec<f32>,
    /// Embedding session for on-demand embedding (kept for query-time embedding).
    pub(super) session: Option<EmbeddingSession>,
}

impl std::fmt::Debug for FlatVecIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatVecIndex")
            .field("dim", &self.dim)
            .field("model_name", &self.model_name)
            .field("format_version", &self.format_version)
            .field("normalized", &self.normalized)
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
pub(super) struct ChunkMeta {
    pub(super) id: ChunkId,
    pub(super) source: EmbeddingChunkSource,
    pub(super) text: String,
}

impl FlatVecIndex {
    /// Build an index from chunks and a model.
    pub fn build(chunks: Vec<EmbeddingChunk>, model: ModelResolution) -> crate::Result<Self> {
        // Load the model
        let session = EmbeddingSession::new_from_resolution(&model)?;

        // Extract texts from chunks
        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();

        // Run inference (Session handles normalization)
        let vectors = session.embed(&texts)?;

        // Convert to flat storage
        let dim = model.embedding_dim() as usize;
        let mut flat_vectors = Vec::with_capacity(chunks.len() * dim);
        for v in vectors {
            flat_vectors.extend(v);
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
            chunks: chunk_metas,
            vectors: flat_vectors,
            session: Some(session),
        })
    }

    /// Embed a text string and return the vector.
    /// Used for query-time embedding during semantic triage.
    pub fn embed_text(&self, text: &str) -> crate::Result<Vec<f32>> {
        let session = self.session.as_ref().ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!(
                "Embedding session not available. Use load_with_resolution() to restore a session."
            ))
        })?;
        let vectors = session.embed(&[text.to_string()])?;
        vectors
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("Failed to embed text")))
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
