//! Flat vector index for embedding similarity search.
//!
//! Stores vectors in a flat array and performs brute-force dot product search.
//! Vectors are pre-normalized during index build.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

use super::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};
use super::model::{EmbeddingSession, ModelResolution};

/// Current format version for the embedding index.
const INDEX_FORMAT_VERSION: u16 = 2;

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
    chunks: Vec<ChunkMeta>,
    /// Vector data as f32 (dim * n_chunks).
    vectors: Vec<f32>,
    /// Embedding session for on-demand embedding (kept for query-time embedding).
    session: Option<EmbeddingSession>,
}

impl Clone for FlatVecIndex {
    fn clone(&self) -> Self {
        Self {
            dim: self.dim,
            model_name: self.model_name.clone(),
            format_version: self.format_version,
            normalized: self.normalized,
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
struct ChunkMeta {
    id: ChunkId,
    source: EmbeddingChunkSource,
    text: String,
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
            format_version: INDEX_FORMAT_VERSION,
            normalized: model.normalize,
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
        let mut file = BufWriter::new(File::create(path)?);

        // Write version and metadata length
        file.write_all(&INDEX_FORMAT_VERSION.to_le_bytes())?;
        let metadata_len = self.chunks.len() as u32;
        file.write_all(&metadata_len.to_le_bytes())?;

        // Write dimension and model name
        file.write_all(&self.dim.to_le_bytes())?;
        let model_name_len = self.model_name.len() as u32;
        file.write_all(&model_name_len.to_le_bytes())?;
        file.write_all(self.model_name.as_bytes())?;

        // Write normalization flag
        file.write_all(&[if self.normalized { 1u8 } else { 0u8 }; 1])?;

        // Write chunks
        for meta in &self.chunks {
            file.write_all(&meta.id.0.to_le_bytes())?;
            match &meta.source {
                EmbeddingChunkSource::Symbol { .. } => file.write_all(&[0u8; 1])?,
                EmbeddingChunkSource::Concept { .. } => file.write_all(&[1u8; 1])?,
            }
            let text_len = meta.text.len() as u32;
            file.write_all(&text_len.to_le_bytes())?;
            file.write_all(meta.text.as_bytes())?;
        }

        // Write vectors
        for v in &self.vectors {
            file.write_all(&v.to_le_bytes())?;
        }

        // Explicit flush so a disk-full / IO failure during the final buffered
        // write surfaces here; BufWriter::drop swallows flush errors.
        file.flush()?;
        Ok(())
    }

    /// Load the index from disk.
    pub fn load(path: &std::path::Path, expected_dim: u16) -> crate::Result<Self> {
        let mut file = BufReader::new(File::open(path)?);
        let mut buf4 = [0u8; 4];
        let mut buf2 = [0u8; 2];

        // Read version
        file.read_exact(&mut buf2)?;
        let version = u16::from_le_bytes(buf2);
        if version != INDEX_FORMAT_VERSION {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "Index format version {} is unsupported (expected {})",
                version,
                INDEX_FORMAT_VERSION
            )));
        }

        // Read metadata length
        file.read_exact(&mut buf4)?;
        let metadata_len = u32::from_le_bytes(buf4) as usize;

        // Read dimension
        file.read_exact(&mut buf2)?;
        let dim = u16::from_le_bytes(buf2);
        if dim != expected_dim {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "Index dimension {} does not match expected {}",
                dim,
                expected_dim
            )));
        }

        // Read model name
        file.read_exact(&mut buf4)?;
        let model_name_len = u32::from_le_bytes(buf4) as usize;
        let mut model_name_buf = vec![0u8; model_name_len];
        file.read_exact(&mut model_name_buf)?;
        let model_name = String::from_utf8(model_name_buf)
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("Invalid model name: {}", e)))?;

        // Read normalization flag
        let mut norm_buf = [0u8; 1];
        file.read_exact(&mut norm_buf)?;
        let normalized = norm_buf[0] != 0;

        // Read chunks
        let mut chunks = Vec::with_capacity(metadata_len);
        let mut buf8 = [0u8; 8];
        for _ in 0..metadata_len {
            file.read_exact(&mut buf8)?;
            let chunk_id = u64::from_le_bytes(buf8);

            let mut variant_tag = [0u8; 1];
            file.read_exact(&mut variant_tag)?;

            file.read_exact(&mut buf4)?;
            let text_len = u32::from_le_bytes(buf4) as usize;
            let mut text_buf = vec![0u8; text_len];
            file.read_exact(&mut text_buf)?;
            let text = String::from_utf8(text_buf)
                .map_err(|e| crate::Error::Other(anyhow::anyhow!("Invalid chunk text: {}", e)))?;

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
            format_version: version,
            normalized,
            chunks,
            vectors,
            session: None,
        })
    }

    /// Load the index and restore the embedding session from a model resolution.
    pub fn load_with_resolution(
        path: &std::path::Path,
        expected_dim: u16,
        model_res: &ModelResolution,
    ) -> crate::Result<Self> {
        let mut index = Self::load(path, expected_dim)?;
        index.session = Some(EmbeddingSession::new_from_resolution(model_res)?);
        Ok(index)
    }

    /// Query the index for top-K similar chunks.
    ///
    /// Uses a bounded min-heap to avoid allocating a score entry for every
    /// chunk. Memory scales with `top_k`, not with the total chunk count.
    /// Ranking semantics are identical to brute-force full sort + take.
    pub fn query(&self, query_vector: &[f32], top_k: usize) -> Vec<(ChunkId, f32)> {
        if self.chunks.is_empty() || query_vector.len() != self.dim as usize {
            return Vec::new();
        }

        // BinaryHeap is max-heap by default. We use Reverse to get min-heap
        // behavior so the smallest-score entry is at the top and gets evicted
        // when the heap is full and a better candidate arrives.
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;

        let mut heap: BinaryHeap<Reverse<(OrderedFloat, usize)>> =
            BinaryHeap::with_capacity(top_k + 1);

        for (i, _chunk) in self.chunks.iter().enumerate() {
            let chunk_vec = &self.vectors[i * self.dim as usize..(i + 1) * self.dim as usize];
            let score = if self.normalized {
                dot_product(query_vector, chunk_vec)
            } else {
                cosine_similarity(query_vector, chunk_vec)
            };

            let entry = Reverse((OrderedFloat(score), i));
            if heap.len() < top_k {
                heap.push(entry);
            } else if entry < *heap.peek().unwrap() {
                // New score exceeds the current minimum: evict the min.
                // Reverse wrapper: entry < peek means inner score > peek's inner score.
                heap.pop();
                heap.push(entry);
            }
        }

        // Extract and sort descending.
        let mut results: Vec<(OrderedFloat, usize)> =
            heap.into_iter().map(|Reverse((f, i))| (f, i)).collect();
        results.sort_by(|a, b| b.0.cmp(&a.0));

        results
            .into_iter()
            .map(|(OrderedFloat(score), i)| (self.chunks[i].id.clone(), score))
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

/// Compute cosine similarity between two vectors (for legacy unnormalized indices).
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let a_norm = (a.iter().map(|x| x * x).sum::<f32>()).sqrt();
    let b_norm = (b.iter().map(|x| x * x).sum::<f32>()).sqrt();
    if a_norm < 1e-10 || b_norm < 1e-10 {
        return 0.0;
    }
    let dot = dot_product(a, b);
    dot / (a_norm * b_norm)
}

/// Wrapper around `f32` that implements `Ord` for use in `BinaryHeap`.
/// Uses total ordering: NaN sorts below all other values.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OrderedFloat(f32);

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .partial_cmp(&other.0)
            .unwrap_or(std::cmp::Ordering::Less)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_returns_empty_for_empty_index() {
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test".into(),
            format_version: INDEX_FORMAT_VERSION,
            normalized: true,
            chunks: vec![],
            vectors: vec![],
            session: None,
        };
        let result = index.query(&[0.0f32; 384], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn persistence_v2_round_trip() -> crate::Result<()> {
        let index = FlatVecIndex {
            dim: 384,
            model_name: "test-model".into(),
            format_version: INDEX_FORMAT_VERSION,
            normalized: true,
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
            vectors: vec![0.1f32; 384],
            session: None,
        };

        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join("index.bin");
        index.save(&path)?;

        let loaded = FlatVecIndex::load(&path, 384)?;
        assert_eq!(loaded.format_version, INDEX_FORMAT_VERSION);
        assert_eq!(loaded.normalized, true);
        assert_eq!(loaded.model_name, "test-model");
        assert_eq!(loaded.len(), 1);

        Ok(())
    }

    /// Verify bounded top-k returns the same results as brute-force full sort.
    #[test]
    fn bounded_top_k_matches_full_sort() {
        let dim = 4usize;
        let n_chunks = 100;

        // Build 100 chunks with known vectors: chunk i has vector [i as f32, 0, 0, 0].
        let chunks: Vec<ChunkMeta> = (0..n_chunks)
            .map(|i| ChunkMeta {
                id: ChunkId(i as u64),
                source: EmbeddingChunkSource::Symbol {
                    id: crate::core::ids::SymbolNodeId(i as u64),
                    file_id: crate::core::ids::FileNodeId(1),
                    qualified_name: format!("test::func{i}"),
                    kind_label: "function".into(),
                },
                text: format!("func{i}"),
            })
            .collect();

        let vectors: Vec<f32> = (0..n_chunks)
            .flat_map(|i| {
                let mut v = vec![0.0f32; dim];
                v[0] = i as f32;
                v
            })
            .collect();

        let index = FlatVecIndex {
            dim: dim as u16,
            model_name: "test".into(),
            format_version: INDEX_FORMAT_VERSION,
            normalized: true,
            chunks,
            vectors,
            session: None,
        };

        // Query with vector [1, 0, 0, 0]: dot product = chunk[i][0] = i.
        // So the top-5 should be chunks 99, 98, 97, 96, 95 in that order.
        let query = vec![1.0f32, 0.0, 0.0, 0.0];
        let top_k = 5;
        let results = index.query(&query, top_k);

        assert_eq!(results.len(), top_k);
        // Descending order: highest dot product first.
        assert_eq!(results[0].0, ChunkId(99));
        assert_eq!(results[0].1, 99.0);
        assert_eq!(results[4].0, ChunkId(95));
        assert_eq!(results[4].1, 95.0);
    }
}
