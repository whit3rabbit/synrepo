//! Similarity scoring (dot product / cosine) and bounded top-K query.

use super::super::chunk::ChunkId;
use super::FlatVecIndex;

impl FlatVecIndex {
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
    dot / f32::max(a_norm * b_norm, f32::EPSILON)
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
    use super::super::persistence::INDEX_FORMAT_VERSION;
    use super::super::ChunkMeta;
    use super::*;
    use crate::substrate::embedding::chunk::EmbeddingChunkSource;

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

    /// Verify bounded top-k returns the same results as brute-force full sort.
    #[test]
    fn bounded_top_k_matches_full_sort() {
        let dim = 4usize;
        let n_chunks = 100;

        let chunks: Vec<ChunkMeta> = (0..n_chunks)
            .map(|i| ChunkMeta {
                id: ChunkId(i as u128),
                source: EmbeddingChunkSource::Symbol {
                    id: crate::core::ids::SymbolNodeId(i as u128),
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

        let query = vec![1.0f32, 0.0, 0.0, 0.0];
        let top_k = 5;
        let results = index.query(&query, top_k);

        assert_eq!(results.len(), top_k);
        assert_eq!(results[0].0, ChunkId(99));
        assert_eq!(results[0].1, 99.0);
        assert_eq!(results[4].0, ChunkId(95));
        assert_eq!(results[4].1, 95.0);
    }
}
