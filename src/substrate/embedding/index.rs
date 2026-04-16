//! Flat vector index for embedding similarity search.
//!
//! Stores vectors in a flat array and performs brute-force cosine similarity search.

use std::fs::File;
use std::io::{Read, Write};

use super::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};
use super::model::{EmbeddingSession, ModelResolution};

/// A flat vector index for cosine similarity search.
pub struct FlatVecIndex {
    /// Vector dimension.
    pub dim: u16,
    /// Model name (for metadata).
    pub model_name: String,
    /// The chunk data (IDs and source info).
    chunks: Vec<ChunkMeta>,
    /// Vector data as f32 (dim * n_chunks).
    vectors: Vec<f32>,
    /// Embedding session for on-demand embedding (kept for query-time embedding).
    #[allow(dead_code)]
    session: Option<EmbeddingSession>,
}

impl Clone for FlatVecIndex {
    fn clone(&self) -> Self {
        Self {
            dim: self.dim,
            model_name: self.model_name.clone(),
            chunks: self.chunks.clone(),
            vectors: self.vectors.clone(),
            session: None, // Don't clone the session
        }
    }
}

impl std::fmt::Debug for FlatVecIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatVecIndex")
            .field("dim", &self.dim)
            .field("model_name", &self.model_name)
            .field("chunks", &self.chunks)
            .field("vectors", &"[...]")
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
struct ChunkMeta {
    id: ChunkId,
    source: EmbeddingChunkSource,
    text: String,
}

impl FlatVecIndex {
    /// Build an index from chunks and a model.
    pub fn build(
        chunks: Vec<EmbeddingChunk>,
        model: ModelResolution,
        _vectors_dir: &std::path::Path,
    ) -> crate::Result<Self> {
        // Load the model
        let session = EmbeddingSession::new(&model.model_path)?;

        // Extract texts from chunks
        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();

        // Run inference
        let vectors = session.embed(&texts)?;

        // Convert to flat storage
        let dim = model.embedding_dim as usize;
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
            dim: model.embedding_dim,
            model_name: model.model_name,
            chunks: chunk_metas,
            vectors: flat_vectors,
            session: Some(session),
        })
    }

    /// Embed a text string and return the vector.
    /// Used for query-time embedding during semantic triage.
    pub fn embed_text(&self, text: &str) -> crate::Result<Vec<f32>> {
        let session = self.session.as_ref().ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!("Embedding session not available"))
        })?;
        let vectors = session.embed(&[text.to_string()])?;
        vectors
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("Failed to embed text")))
    }

    /// Get a reference to symbol chunk text by its ChunkId.
    /// Returns the text if the chunk is a symbol.
    pub fn symbol_chunk_text(&self, chunk_id: &ChunkId) -> Option<&str> {
        self.chunks
            .iter()
            .find(|c| c.id == *chunk_id)
            .and_then(|c| {
                if matches!(c.source, EmbeddingChunkSource::Symbol { .. }) {
                    Some(c.text.as_str())
                } else {
                    None
                }
            })
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

    /// Save the index to disk.
    pub fn save(&self, path: &std::path::Path) -> crate::Result<()> {
        let mut file = File::create(path)?;
        let metadata_len = self.chunks.len() as u32;
        file.write_all(&metadata_len.to_le_bytes())?;

        // Write dimension and model name
        file.write_all(&self.dim.to_le_bytes())?;
        let model_name_len = self.model_name.len() as u32;
        file.write_all(&model_name_len.to_le_bytes())?;
        file.write_all(self.model_name.as_bytes())?;

        // Write chunks
        for meta in &self.chunks {
            file.write_all(&meta.id.0.to_le_bytes())?;
            // Write source (simplified - just the variant tag)
            match &meta.source {
                EmbeddingChunkSource::Symbol { .. } => file.write_all(&[0u8; 1])?,
                EmbeddingChunkSource::Concept { .. } => file.write_all(&[1u8; 1])?,
            }
            // Write text length and text
            let text_len = meta.text.len() as u32;
            file.write_all(&text_len.to_le_bytes())?;
            file.write_all(meta.text.as_bytes())?;
        }

        // Write vectors
        for v in &self.vectors {
            let bits = v.to_le_bytes();
            file.write_all(&bits)?;
        }

        Ok(())
    }

    /// Load the index from disk.
    pub fn load(path: &std::path::Path, expected_dim: u16) -> crate::Result<Self> {
        let mut file = File::open(path)?;
        let mut buf = [0u8; 4];

        // Read metadata length
        file.read_exact(&mut buf[..4])?;
        let metadata_len = u32::from_le_bytes(buf) as usize;

        // Read dimension
        file.read_exact(&mut buf[..2])?;
        let dim = u16::from_le_bytes([buf[0], buf[1]]);
        if dim != expected_dim {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "Index dimension {} does not match expected {}",
                dim,
                expected_dim
            )));
        }

        // Read model name
        file.read_exact(&mut buf[..4])?;
        let model_name_len = u32::from_le_bytes(buf) as usize;
        let mut model_name_buf = vec![0u8; model_name_len];
        file.read_exact(&mut model_name_buf)?;
        let model_name = String::from_utf8(model_name_buf)
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Invalid model name: {}", e)))?;

        // Read chunks
        let mut chunks = Vec::with_capacity(metadata_len);
        let mut buf8 = [0u8; 8];
        for _ in 0..metadata_len {
            file.read_exact(&mut buf8)?;
            let chunk_id = u64::from_le_bytes(buf8);

            let mut variant_tag = [0u8; 1];
            file.read_exact(&mut variant_tag)?;

            file.read_exact(&mut buf[..4])?;
            let text_len = u32::from_le_bytes(buf) as usize;
            let mut text_buf = vec![0u8; text_len];
            file.read_exact(&mut text_buf)?;
            let text = String::from_utf8(text_buf)
                .map_err(|e| crate::Error::Other(anyhow::anyhow!("Invalid chunk text: {}", e)))?;

            // Reconstruct source (simplified)
            let source = if variant_tag[0] == 0 {
                EmbeddingChunkSource::Symbol {
                    id: crate::core::ids::SymbolNodeId(chunk_id),
                    file_id: crate::core::ids::FileNodeId(0),
                    qualified_name: text.clone(),
                    kind_label: String::new(),
                }
            } else {
                EmbeddingChunkSource::Concept {
                    id: crate::core::ids::ConceptNodeId(chunk_id),
                    path: text.clone(),
                }
            };

            chunks.push(ChunkMeta {
                id: ChunkId(chunk_id),
                source,
                text,
            });
        }

        // Read vectors
        let vector_len = metadata_len * dim as usize;
        let mut vectors = vec![0f32; vector_len];
        for v in &mut vectors {
            let mut bits = [0u8; 4];
            file.read_exact(&mut bits)?;
            *v = f32::from_le_bytes(bits);
        }

        Ok(Self {
            dim,
            model_name,
            chunks,
            vectors,
            session: None,
        })
    }

    /// Load the index and optionally restore the embedding session for query-time embedding.
    pub fn load_with_model_path(
        path: &std::path::Path,
        expected_dim: u16,
        model_path: &std::path::Path,
    ) -> crate::Result<Self> {
        let mut index = Self::load(path, expected_dim)?;
        index.session = Some(EmbeddingSession::new(model_path)?);
        Ok(index)
    }

    /// Query the index for top-K similar chunks.
    pub fn query(&self, query_vector: &[f32], top_k: usize) -> Vec<(ChunkId, f32)> {
        if self.chunks.is_empty() || query_vector.len() != self.dim as usize {
            return Vec::new();
        }

        // Compute cosine similarity with all vectors
        let mut scores: Vec<(usize, f32)> = Vec::with_capacity(self.chunks.len());

        // Normalize query vector
        let query_norm = dot_product(query_vector, query_vector).sqrt();
        if query_norm < 1e-10 {
            return Vec::new();
        }

        for (i, _chunk) in self.chunks.iter().enumerate() {
            let chunk_vec = &self.vectors[i * self.dim as usize..(i + 1) * self.dim as usize];
            let similarity = cosine_similarity(query_vector, chunk_vec, query_norm);
            scores.push((i, similarity));
        }

        // Sort by similarity descending
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top K
        scores
            .into_iter()
            .take(top_k)
            .map(|(i, score)| (self.chunks[i].id.clone(), score))
            .collect()
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

/// Compute dot product of two vectors.
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32], a_norm: f32) -> f32 {
    let b_norm = (b.iter().map(|x| x * x).sum::<f32>()).sqrt();
    if b_norm < 1e-10 {
        return 0.0;
    }
    let dot = dot_product(a, b);
    dot / (a_norm * b_norm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::substrate::embedding::ChunkId;
    use std::io::Seek;

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        let result = cosine_similarity(&v, &v, 1.0);
        assert!((result - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let result = cosine_similarity(&a, &b, 1.0);
        assert!(result.abs() < 1e-6);
    }

    #[test]
    fn query_returns_empty_for_empty_index() {
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![],
            vectors: vec![],
            session: None,
        };
        let result = index.query(&[0.0f32; 384], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn query_returns_empty_for_wrong_dimension() {
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![],
            vectors: vec![],
            session: None,
        };
        // Query with wrong dimension
        let result = index.query(&[0.0f32; 256], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn len_and_is_empty_work() {
        let empty_index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![],
            vectors: vec![],
            session: None,
        };
        assert!(empty_index.is_empty());
        assert_eq!(empty_index.len(), 0);

        let index_with_chunks = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![ChunkMeta {
                id: ChunkId(1),
                source: EmbeddingChunkSource::Symbol {
                    id: crate::core::ids::SymbolNodeId(1),
                    file_id: crate::core::ids::FileNodeId(1),
                    qualified_name: "test::func".into(),
                    kind_label: "function".into(),
                },
                text: "test::func function".into(),
            }],
            vectors: vec![0.0f32; 384],
            session: None,
        };
        assert!(!index_with_chunks.is_empty());
        assert_eq!(index_with_chunks.len(), 1);
    }

    #[test]
    fn persistence_round_trip() -> crate::Result<()> {
        // Create an index with some chunks
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test-model".into(),
            chunks: vec![
                ChunkMeta {
                    id: ChunkId(1),
                    source: EmbeddingChunkSource::Symbol {
                        id: crate::core::ids::SymbolNodeId(1),
                        file_id: crate::core::ids::FileNodeId(1),
                        qualified_name: "test::func".into(),
                        kind_label: "function".into(),
                    },
                    text: "test::func function".into(),
                },
                ChunkMeta {
                    id: ChunkId(2),
                    source: EmbeddingChunkSource::Concept {
                        id: crate::core::ids::ConceptNodeId(1),
                        path: "docs/concepts/test.md".into(),
                    },
                    text: "Test concept".into(),
                },
            ],
            vectors: vec![0.1f32; 384 * 2], // Two vectors
            session: None,
        };

        // Save to a temp file
        let mut temp_file = tempfile::NamedTempFile::new()?;
        index.save(temp_file.path())?;

        // Load it back
        temp_file.seek(std::io::SeekFrom::Start(0))?;
        let loaded = FlatVecIndex::load(temp_file.path(), 384)?;

        // Verify
        assert_eq!(loaded.dim, 384);
        assert_eq!(loaded.model_name, "test-model");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.vectors.len(), 384 * 2);

        // Verify chunks
        if let EmbeddingChunkSource::Symbol { id, .. } = &loaded.chunks[0].source {
            assert_eq!(*id, crate::core::ids::SymbolNodeId(1));
        } else {
            panic!("Expected Symbol chunk");
        }

        if let EmbeddingChunkSource::Concept { id, .. } = &loaded.chunks[1].source {
            assert_eq!(*id, crate::core::ids::ConceptNodeId(1));
        } else {
            panic!("Expected Concept chunk");
        }

        Ok(())
    }

    #[test]
    fn load_validates_dimension() {
        // Create and save an index with dim 384
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![],
            vectors: vec![],
            session: None,
        };

        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        index.save(temp_file.path()).unwrap();

        // Try to load with wrong dimension
        let result = FlatVecIndex::load(temp_file.path(), 768);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("dimension"));
    }

    #[test]
    fn chunk_to_symbol_id_returns_correct_type() {
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![ChunkMeta {
                id: ChunkId(42),
                source: EmbeddingChunkSource::Symbol {
                    id: crate::core::ids::SymbolNodeId(42),
                    file_id: crate::core::ids::FileNodeId(1),
                    qualified_name: "test::func".into(),
                    kind_label: "function".into(),
                },
                text: "test::func function".into(),
            }],
            vectors: vec![0.0f32; 384],
            session: None,
        };

        let result = index.chunk_to_symbol_id(&ChunkId(42));
        assert!(result.is_some());
        assert_eq!(result.unwrap(), crate::core::ids::SymbolNodeId(42));

        // Non-symbol chunk should return None
        let concept_index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![ChunkMeta {
                id: ChunkId(100),
                source: EmbeddingChunkSource::Concept {
                    id: crate::core::ids::ConceptNodeId(100),
                    path: "docs/concepts/test.md".into(),
                },
                text: "Test concept".into(),
            }],
            vectors: vec![0.0f32; 384],
            session: None,
        };

        let result = concept_index.chunk_to_symbol_id(&ChunkId(100));
        assert!(result.is_none());
    }

    #[test]
    fn symbol_chunk_text_returns_correct_text() {
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![ChunkMeta {
                id: ChunkId(1),
                source: EmbeddingChunkSource::Symbol {
                    id: crate::core::ids::SymbolNodeId(1),
                    file_id: crate::core::ids::FileNodeId(1),
                    qualified_name: "my_module::my_function".into(),
                    kind_label: "function".into(),
                },
                text: "my_module::my_function function".into(),
            }],
            vectors: vec![0.0f32; 384],
            session: None,
        };

        let result = index.symbol_chunk_text(&ChunkId(1));
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "my_module::my_function function");

        // Concept chunk should return None
        let concept_index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![ChunkMeta {
                id: ChunkId(2),
                source: EmbeddingChunkSource::Concept {
                    id: crate::core::ids::ConceptNodeId(2),
                    path: "docs/concepts/test.md".into(),
                },
                text: "Test concept".into(),
            }],
            vectors: vec![0.0f32; 384],
            session: None,
        };

        let result = concept_index.symbol_chunk_text(&ChunkId(2));
        assert!(result.is_none());
    }

    #[test]
    fn clone_does_not_clone_session() {
        // Create index without session (session is None after build/load)
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            chunks: vec![],
            vectors: vec![1.0f32; 384],
            session: None,
        };

        let cloned = index.clone();
        // Session should be None in cloned version (by design)
        assert!(cloned.session.is_none());
        // But other data should be cloned
        assert_eq!(cloned.dim, 384);
        assert_eq!(cloned.vectors.len(), 384);
    }
}
