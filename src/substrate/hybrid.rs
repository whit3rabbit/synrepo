//! Hybrid lexical plus semantic search helpers.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use syntext::SearchOptions;

use crate::config::Config;

const LEXICAL_TOP_K: usize = 100;
#[cfg(feature = "semantic-triage")]
const SEMANTIC_TOP_K: usize = 50;
const RRF_K: f32 = 60.0;

/// Source lanes that contributed to a search row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridSearchSource {
    /// Row came from lexical search only.
    Lexical,
    /// Row came from vector search only.
    Semantic,
    /// Row was found by both lanes.
    Hybrid,
}

impl HybridSearchSource {
    /// Stable user-facing label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}

/// One fused search result row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridSearchRow {
    /// Repo-relative path when known.
    pub path: Option<String>,
    /// 1-based line number for lexical rows.
    pub line: Option<u32>,
    /// Line content for lexical rows.
    pub content: Option<String>,
    /// Source lane.
    pub source: HybridSearchSource,
    /// Reciprocal-rank fusion score.
    pub fusion_score: f32,
    /// Raw semantic similarity score when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_score: Option<f32>,
    /// Semantic chunk identifier when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
    /// Symbol node identifier for symbol chunks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<crate::core::ids::SymbolNodeId>,
}

/// Hybrid search output before API-specific rendering.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridSearchReport {
    /// Results after RRF and final limit.
    pub rows: Vec<HybridSearchRow>,
    /// True when semantic search contributed rows.
    pub semantic_available: bool,
    /// Engine label for response metadata.
    pub engine: &'static str,
}

#[derive(Clone, Debug)]
struct Accumulator {
    row: HybridSearchRow,
    best_rank: usize,
}

/// Run hybrid lexical plus semantic search.
///
/// Semantic loading is best-effort and local-only. Missing vector indexes or
/// model artifacts return lexical results instead of surfacing an error.
pub fn hybrid_search(
    config: &Config,
    repo_root: &std::path::Path,
    query: &str,
    options: &SearchOptions,
) -> crate::Result<HybridSearchReport> {
    let final_limit = options.max_results.unwrap_or(20);
    let mut lexical_options = options.clone();
    lexical_options.max_results = Some(LEXICAL_TOP_K.max(final_limit));
    let lexical =
        crate::substrate::index::search_with_options(config, repo_root, query, &lexical_options)?;

    let mut rows = HashMap::<String, Accumulator>::new();
    for (rank, item) in lexical.into_iter().enumerate() {
        let path = item.path.to_string_lossy().into_owned();
        let content = String::from_utf8_lossy(&item.line_content)
            .trim_end()
            .to_string();
        let key = format!("lexical:{path}:{}:{content}", item.line_number);
        rows.insert(
            key,
            Accumulator {
                row: HybridSearchRow {
                    path: Some(path),
                    line: Some(item.line_number),
                    content: Some(content),
                    source: HybridSearchSource::Lexical,
                    fusion_score: rrf(rank),
                    semantic_score: None,
                    chunk_id: None,
                    symbol_id: None,
                },
                best_rank: rank,
            },
        );
    }

    let semantic_available = add_semantic_rows(config, repo_root, query, &mut rows);
    let mut fused = rows.into_values().collect::<Vec<_>>();
    fused.sort_by(|a, b| {
        b.row
            .fusion_score
            .partial_cmp(&a.row.fusion_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.best_rank.cmp(&b.best_rank))
    });
    fused.truncate(final_limit);

    Ok(HybridSearchReport {
        rows: fused.into_iter().map(|acc| acc.row).collect(),
        semantic_available,
        engine: if semantic_available {
            "syntext+vectors"
        } else {
            "syntext"
        },
    })
}

#[cfg(not(feature = "semantic-triage"))]
fn add_semantic_rows(
    _config: &Config,
    _repo_root: &std::path::Path,
    _query: &str,
    _rows: &mut HashMap<String, Accumulator>,
) -> bool {
    false
}

#[cfg(feature = "semantic-triage")]
fn add_semantic_rows(
    config: &Config,
    repo_root: &std::path::Path,
    query: &str,
    rows: &mut HashMap<String, Accumulator>,
) -> bool {
    if !config.enable_semantic_triage {
        return false;
    }
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let Ok(Some(index)) = crate::substrate::embedding::load_embedding_index(config, &synrepo_dir)
    else {
        return false;
    };
    let Ok(query_vec) = index.embed_text(query) else {
        return false;
    };
    let semantic = index.query(&query_vec, SEMANTIC_TOP_K);
    if semantic.is_empty() {
        return true;
    }

    for (rank, (chunk_id, score)) in semantic.into_iter().enumerate() {
        let key = format!("semantic:{chunk_id}");
        let fusion = rrf(rank);
        let symbol_id = index.chunk_to_symbol_id(&chunk_id);
        rows.entry(key)
            .and_modify(|acc| {
                acc.row.source = HybridSearchSource::Hybrid;
                acc.row.fusion_score += fusion;
                acc.row.semantic_score = Some(score);
                acc.row.chunk_id = Some(chunk_id.to_string());
                acc.row.symbol_id = symbol_id;
                acc.best_rank = acc.best_rank.min(rank);
            })
            .or_insert_with(|| Accumulator {
                row: HybridSearchRow {
                    path: None,
                    line: None,
                    content: None,
                    source: HybridSearchSource::Semantic,
                    fusion_score: fusion,
                    semantic_score: Some(score),
                    chunk_id: Some(chunk_id.to_string()),
                    symbol_id,
                },
                best_rank: rank + LEXICAL_TOP_K,
            });
    }
    true
}

fn rrf(rank_zero_based: usize) -> f32 {
    1.0 / (RRF_K + rank_zero_based as f32 + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn hybrid_search_falls_back_to_lexical_without_semantic_assets() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".synrepo/index")).unwrap();
        fs::write(repo.path().join("README.md"), "alpha token\n").unwrap();
        let config = Config::default();
        crate::substrate::index::build_index(&config, repo.path()).unwrap();

        let report =
            hybrid_search(&config, repo.path(), "alpha", &SearchOptions::default()).unwrap();
        assert!(!report.semantic_available);
        assert_eq!(report.engine, "syntext");
        assert_eq!(report.rows[0].source, HybridSearchSource::Lexical);
    }
}
