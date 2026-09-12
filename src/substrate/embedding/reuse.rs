//! Incremental embedding vector reuse partition.
//!
//! Matches freshly extracted chunks against an existing `FlatVecIndex` using
//! `(ChunkId, blake3(text))` keys. Unchanged chunks reuse their previously computed
//! vectors, avoiding repeated neural inference on unchanged code.

use std::collections::HashMap;

use super::chunk::{ChunkId, EmbeddingChunk};
use super::FlatVecIndex;

/// Blake3 hash of chunk text.
pub type ChunkTextHash = [u8; 32];

/// Compute blake3 hash of chunk text.
pub fn hash_text(text: &str) -> ChunkTextHash {
    *blake3::hash(text.as_bytes()).as_bytes()
}

/// Partition result describing vector reuse from an existing index.
pub struct VectorReuse<'a> {
    /// For each chunk in `fresh_chunks`, either a borrowed vector slice from
    /// the old index or `None` if the chunk is new or modified.
    pub chunk_vectors: Vec<Option<&'a [f32]>>,
    /// Number of reused chunks.
    pub reused_count: usize,
    /// Number of miss chunks needing inference.
    pub miss_count: usize,
}

impl<'a> VectorReuse<'a> {
    /// Whether any chunks need embedding inference.
    pub fn has_misses(&self) -> bool {
        self.miss_count > 0
    }

    /// Whether every fresh chunk was reused and the total count matches the old index.
    pub fn is_exact_match(&self, old_len: usize) -> bool {
        self.miss_count == 0 && self.chunk_vectors.len() == old_len
    }
}

/// Partition `fresh_chunks` against `old_index` by `(ChunkId, blake3(text))`.
pub fn partition_chunks<'a>(
    fresh_chunks: &[EmbeddingChunk],
    old_index: &'a FlatVecIndex,
) -> VectorReuse<'a> {
    // Build index of old chunks: (ChunkId, text_hash) -> queue of vector slices.
    // A queue/Vec handles duplicate (id, hash) pairs gracefully.
    let mut old_map: HashMap<(ChunkId, ChunkTextHash), Vec<&'a [f32]>> = HashMap::new();
    for (id, text, vector) in old_index.iter_chunks_with_vectors() {
        let hash = hash_text(text);
        old_map.entry((*id, hash)).or_default().push(vector);
    }

    let mut chunk_vectors = Vec::with_capacity(fresh_chunks.len());
    let mut reused_count = 0;
    let mut miss_count = 0;

    for chunk in fresh_chunks {
        let hash = hash_text(&chunk.text);
        let key = (chunk.id, hash);
        let maybe_vec = match old_map.get_mut(&key) {
            Some(vecs) if !vecs.is_empty() => {
                reused_count += 1;
                Some(vecs.remove(0))
            }
            _ => {
                miss_count += 1;
                None
            }
        };
        chunk_vectors.push(maybe_vec);
    }

    VectorReuse {
        chunk_vectors,
        reused_count,
        miss_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::substrate::embedding::chunk::EmbeddingChunkSource;

    fn sample_chunk(id_val: u128, text: &str) -> EmbeddingChunk {
        EmbeddingChunk {
            id: ChunkId(id_val),
            source: EmbeddingChunkSource::Concept {
                id: crate::core::ids::ConceptNodeId(1),
                path: "docs/test.md".to_string(),
            },
            text: text.to_string(),
        }
    }

    fn build_test_index(chunks: Vec<EmbeddingChunk>, vectors: Vec<f32>) -> FlatVecIndex {
        let chunk_metas = chunks
            .into_iter()
            .map(|c| crate::substrate::embedding::index::ChunkMeta {
                id: c.id,
                source: c.source,
                text: c.text,
            })
            .collect();
        FlatVecIndex {
            dim: 2,
            model_name: "test-model".to_string(),
            format_version: 6,
            normalized: true,
            precision: crate::substrate::embedding::profile::VectorPrecision::Float32,
            normalizer_version: 1,
            chunks: chunk_metas,
            vectors,
            session: None,
        }
    }

    #[test]
    fn partition_all_reused_on_exact_match() {
        let chunks = vec![
            sample_chunk(1, "function foo() {}"),
            sample_chunk(2, "function bar() {}"),
        ];
        let old_index = build_test_index(chunks.clone(), vec![1.0, 0.0, 0.0, 1.0]);

        let partition = partition_chunks(&chunks, &old_index);
        assert_eq!(partition.reused_count, 2);
        assert_eq!(partition.miss_count, 0);
        assert!(partition.is_exact_match(2));
        assert_eq!(partition.chunk_vectors[0].unwrap(), &[1.0, 0.0]);
        assert_eq!(partition.chunk_vectors[1].unwrap(), &[0.0, 1.0]);
    }

    #[test]
    fn partition_detects_changed_chunk_as_miss() {
        let old_chunks = vec![
            sample_chunk(1, "function foo() {}"),
            sample_chunk(2, "function bar() {}"),
        ];
        let old_index = build_test_index(old_chunks, vec![1.0, 0.0, 0.0, 1.0]);

        let fresh_chunks = vec![
            sample_chunk(1, "function foo() { /* edited */ }"),
            sample_chunk(2, "function bar() {}"),
        ];

        let partition = partition_chunks(&fresh_chunks, &old_index);
        assert_eq!(partition.reused_count, 1);
        assert_eq!(partition.miss_count, 1);
        assert!(!partition.is_exact_match(2));
        assert!(partition.chunk_vectors[0].is_none());
        assert_eq!(partition.chunk_vectors[1].unwrap(), &[0.0, 1.0]);
    }

    #[test]
    fn partition_handles_duplicate_texts_with_differing_ids() {
        let old_chunks = vec![
            sample_chunk(1, "duplicate text"),
            sample_chunk(2, "duplicate text"),
        ];
        let old_index = build_test_index(old_chunks, vec![1.0, 0.0, 0.0, 1.0]);

        let fresh_chunks = vec![
            sample_chunk(1, "duplicate text"),
            sample_chunk(2, "duplicate text"),
        ];

        let partition = partition_chunks(&fresh_chunks, &old_index);
        assert_eq!(partition.reused_count, 2);
        assert_eq!(partition.miss_count, 0);
        assert_eq!(partition.chunk_vectors[0].unwrap(), &[1.0, 0.0]);
        assert_eq!(partition.chunk_vectors[1].unwrap(), &[0.0, 1.0]);
    }

    #[test]
    fn partition_handles_removals() {
        let old_chunks = vec![sample_chunk(1, "keep me"), sample_chunk(2, "remove me")];
        let old_index = build_test_index(old_chunks, vec![1.0, 0.0, 0.0, 1.0]);

        let fresh_chunks = vec![sample_chunk(1, "keep me")];

        let partition = partition_chunks(&fresh_chunks, &old_index);
        assert_eq!(partition.reused_count, 1);
        assert_eq!(partition.miss_count, 0);
        // Not exact match because count changed from 2 to 1
        assert!(!partition.is_exact_match(2));
        assert_eq!(partition.chunk_vectors[0].unwrap(), &[1.0, 0.0]);
    }
}
