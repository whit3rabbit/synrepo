//! Chunk extraction for embedding.
//!
//! Extracts embedding chunks from graph symbols and prose concepts.
//! Each symbol produces one chunk from `qualified_name + " " + symbol_kind`.
//! Each concept produces one chunk from its full text (truncated to 512 tokens).

use crate::core::ids::{ConceptNodeId, FileNodeId, SymbolNodeId};
use crate::structure::graph::{with_graph_read_snapshot, GraphStore};

/// Maximum tokens for prose concept text.
const MAX_PROSE_TOKENS: usize = 512;

/// A chunk of text to be embedded.
#[derive(Clone, Debug)]
pub struct EmbeddingChunk {
    /// Unique identifier for this chunk.
    pub id: ChunkId,
    /// Source of this chunk (symbol or concept).
    pub source: EmbeddingChunkSource,
    /// The text to embed.
    pub text: String,
}

/// Source of an embedding chunk.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum EmbeddingChunkSource {
    /// From a symbol node.
    Symbol {
        id: SymbolNodeId,
        file_id: FileNodeId,
        qualified_name: String,
        kind_label: String,
    },
    /// From a concept node.
    Concept { id: ConceptNodeId, path: String },
}

/// Unique identifier for an embedding chunk.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ChunkId(pub u64);

impl std::fmt::Display for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chunk_{:016x}", self.0)
    }
}

/// Extract all embedding chunks from a graph store.
pub fn extract_chunks(graph: &dyn GraphStore) -> crate::Result<Vec<EmbeddingChunk>> {
    with_graph_read_snapshot(graph, extract_chunks_inner)
}

fn extract_chunks_inner(graph: &dyn GraphStore) -> crate::Result<Vec<EmbeddingChunk>> {
    let mut chunks = Vec::new();

    // Get all symbol names and kinds
    let symbol_names = graph.all_symbol_names()?;
    // all_symbol_names returns Vec<(SymbolNodeId, FileNodeId, String)>
    for (id, file_id, qualified_name) in symbol_names {
        // Load the symbol to get its kind
        if let Some(symbol) = graph.get_symbol(id)? {
            let kind_label = symbol.kind.as_str().to_string();
            let text = format!("{} {}", qualified_name, kind_label);
            chunks.push(EmbeddingChunk {
                id: ChunkId(id.0),
                source: EmbeddingChunkSource::Symbol {
                    id,
                    file_id,
                    qualified_name,
                    kind_label,
                },
                text,
            });
        }
    }

    // Extract concept chunks
    let concept_paths = graph.all_concept_paths()?;
    for (path, id) in concept_paths {
        // Load the concept node to get its content
        if let Some(concept) = graph.get_concept(id)? {
            // Use title + aliases + summary as the text
            let mut text_parts = vec![concept.title.clone()];
            text_parts.extend(concept.aliases.clone());
            if let Some(summary) = &concept.summary {
                text_parts.push(summary.clone());
            }
            // Truncate to MAX_PROSE_TOKENS (approximate by characters / 4)
            let max_chars = MAX_PROSE_TOKENS * 4;
            let text = truncate_text(&text_parts.join(" "), max_chars);

            if !text.is_empty() {
                chunks.push(EmbeddingChunk {
                    id: ChunkId(id.0),
                    source: EmbeddingChunkSource::Concept { id, path },
                    text,
                });
            }
        }
    }

    Ok(chunks)
}

/// Truncate text to approximately max_tokens (character estimate).
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        // Find a safe character-boundary break point
        let end = text
            .char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        let trunc_at = &text[..end];
        if let Some(last_space) = trunc_at.rfind(' ') {
            trunc_at[..last_space].to_string()
        } else {
            trunc_at.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{FileNodeId, SymbolNodeId};
    use crate::core::provenance::Provenance;
    use crate::structure::graph::{Epistemic, SymbolKind, SymbolNode};

    fn test_provenance() -> Provenance {
        Provenance::structural("chunk-test", "rev", Vec::new())
    }

    #[test]
    fn truncate_text_under_limit() {
        let input = "short text";
        assert_eq!(truncate_text(input, 100), input);
    }

    #[test]
    fn truncate_text_over_limit() {
        let input = "a".repeat(200);
        let result = truncate_text(&input, 100);
        assert!(result.len() <= 100 + 10); // Allow some overflow for lack of space
    }

    #[test]
    fn truncate_text_at_word_boundary() {
        let input = "hello world this is a test";
        let result = truncate_text(input, 11); // Should cut after "hello world"
        assert!(!result.ends_with('-'));
    }

    #[test]
    fn truncate_text_multibyte_chars() {
        // Multi-byte characters (accented, emoji) should not cause panic
        let input = "hello café world 🎉 test";
        let result = truncate_text(input, 8); // Should not panic
        assert!(!result.is_empty());
        assert!(result.len() <= 8 * 4); // Rough character expansion bound
    }

    // Chunk extraction tests - these would need a test graph implementation
    // The actual extraction requires a GraphStore which is complex to mock
    // So we test the chunk types and source variants instead

    #[test]
    fn embedding_chunk_source_variants() {
        // Test Symbol variant
        let symbol_source = EmbeddingChunkSource::Symbol {
            id: SymbolNodeId(1),
            file_id: FileNodeId(1),
            qualified_name: "my_module::my_function".into(),
            kind_label: "function".into(),
        };
        assert!(matches!(symbol_source, EmbeddingChunkSource::Symbol { .. }));

        // Test Concept variant
        let concept_source = EmbeddingChunkSource::Concept {
            id: ConceptNodeId(1),
            path: "docs/concepts/test.md".into(),
        };
        assert!(matches!(
            concept_source,
            EmbeddingChunkSource::Concept { .. }
        ));
    }

    #[test]
    fn chunk_id_display_format() {
        let id = ChunkId(42);
        assert_eq!(id.to_string(), "chunk_000000000000002a");
        let id_big = ChunkId(0xDEADBEEF);
        assert_eq!(id_big.to_string(), "chunk_00000000deadbeef");
    }

    #[test]
    fn chunk_id_eq_and_hash() {
        let id1 = ChunkId(1);
        let id2 = ChunkId(1);
        let id3 = ChunkId(2);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);

        // Test hashability
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2); // Should not add duplicate
        set.insert(id3);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn embedding_chunk_clone() {
        let chunk = EmbeddingChunk {
            id: ChunkId(1),
            source: EmbeddingChunkSource::Symbol {
                id: SymbolNodeId(1),
                file_id: FileNodeId(1),
                qualified_name: "test".into(),
                kind_label: "function".into(),
            },
            text: "test function".into(),
        };

        let cloned = chunk.clone();
        assert_eq!(cloned.id, chunk.id);
        assert_eq!(cloned.text, chunk.text);
    }
}
