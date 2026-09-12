//! Dense-first search arm: vector hits ranked by cosine, lexical fallback.
//!
//! This arm is bench-only today (`synrepo bench search --mode dense-first|all`)
//! and is intentionally not wired into `synrepo_search mode=auto`; see
//! `docs/EMBEDDINGS.md` § "Dense-first vs auto (RRF)" for the measured
//! tradeoff against this repository's fixture set.

use syntext::SearchOptions;

use super::{Accumulator, HybridSearchReport, HybridSearchRow, HybridSearchSource};
use crate::config::Config;

/// Run dense-first search: vector hits ranked by cosine, with lexical as the
/// no-vectors fallback.
///
/// If the vector lane returns at least one hit, lexical results are dropped
/// entirely — only the cosine-ranked vector hits appear in the report.
/// Lexical is the fallback when the vector index is missing, the model
/// cannot be loaded, the query fails to embed, or the vector lane returns
/// zero hits. This mirrors the dense-first arm measured against the auto
/// RRF arm in `docs/EMBEDDINGS.md`'s bench, and the contract is the same
/// one `memoryfield` reports in its `BENCHMARKS.md` (section 2): RRF
/// averages FTS keyword noise into dense rankings; dense-first does not.
pub fn dense_first_search(
    config: &Config,
    repo_root: &std::path::Path,
    query: &str,
    options: &SearchOptions,
) -> crate::Result<HybridSearchReport> {
    let final_limit = options.max_results.unwrap_or(20);

    let semantic_rows = dense_vector_rows(config, repo_root, query);

    // Decide upfront whether the vector lane produced rows; the lexical-
    // fallback path below can still match on `semantic_rows` without
    // double-borrowing.
    let vector_lane_produced_rows = matches!(
        semantic_rows.as_ref(),
        Some(rows) if !rows.is_empty()
    );

    let rows: Vec<HybridSearchRow> = match semantic_rows {
        Some(mut rows) if !rows.is_empty() => {
            rows.sort_by(|a, b| {
                b.row
                    .fusion_score
                    .partial_cmp(&a.row.fusion_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.best_rank.cmp(&b.best_rank))
            });
            rows.truncate(final_limit);
            rows.into_iter().map(|acc| acc.row).collect()
        }
        // Either no vector config, no cached index, embedding failed, or the
        // vector lane returned zero hits. Fall back to lexical.
        _ => {
            let mut lexical_options = options.clone();
            lexical_options.max_results = Some(final_limit);
            let lexical = crate::substrate::search_rooted_with_options(
                config,
                repo_root,
                query,
                &lexical_options,
            )?;
            lexical
                .into_iter()
                .take(final_limit)
                .map(|item| HybridSearchRow {
                    path: Some(item.path.to_string_lossy().into_owned()),
                    root_id: Some(item.root_id),
                    is_primary_root: Some(item.is_primary_root),
                    root_kind: Some(item.root_kind),
                    root_label: Some(item.root_label),
                    root_ref: item.root_ref,
                    root_commit: item.root_commit,
                    editable: Some(item.editable),
                    file_id: None,
                    line: Some(item.line_number),
                    content: Some(
                        String::from_utf8_lossy(&item.line_content)
                            .trim_end()
                            .to_string(),
                    ),
                    source: HybridSearchSource::Lexical,
                    fusion_score: 0.0,
                    semantic_score: None,
                    chunk_id: None,
                    symbol_id: None,
                })
                .collect()
        }
    };

    // `semantic_available` reflects whether the vector lane was consulted
    // and produced any non-fallback results. Lexical-fallback reports false
    // so callers can distinguish "no semantic data" from "we tried semantics
    // and they won".
    Ok(HybridSearchReport {
        rows,
        semantic_available: vector_lane_produced_rows,
        engine: if vector_lane_produced_rows {
            "dense-first"
        } else {
            "syntext"
        },
    })
}

/// Vector lane for [`dense_first_search`]. `None` means the lexical fallback
/// must run (semantic off, index/model missing, or embedding failed);
/// `Some(vec![])` means the lane ran but matched nothing.
#[cfg(feature = "semantic-triage")]
fn dense_vector_rows(
    config: &Config,
    repo_root: &std::path::Path,
    query: &str,
) -> Option<Vec<Accumulator>> {
    if !config.enable_semantic_triage {
        return None;
    }
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let index = crate::substrate::embedding::load_embedding_index(config, &synrepo_dir).ok()??;
    let query_vec = index.embed_text(query).ok()?;
    let hits = index.query(&query_vec, super::SEMANTIC_TOP_K);
    if hits.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::with_capacity(hits.len());
    for (rank, (chunk_id, score)) in hits.into_iter().enumerate() {
        let symbol_id = index.chunk_to_symbol_id(&chunk_id);
        let row = HybridSearchRow {
            path: None,
            root_id: None,
            is_primary_root: None,
            root_kind: None,
            root_label: None,
            root_ref: None,
            root_commit: None,
            editable: None,
            file_id: None,
            line: None,
            content: None,
            source: HybridSearchSource::Semantic,
            // In dense-first, the score that drives ranking is the raw
            // cosine score, not RRF. Keep the field name (`fusion_score`)
            // for output-shape compatibility with `hybrid_search`.
            fusion_score: score,
            semantic_score: Some(score),
            chunk_id: Some(chunk_id.to_string()),
            symbol_id,
        };
        out.push(Accumulator {
            row,
            best_rank: rank,
        });
    }
    Some(out)
}

/// Semantic triage is not compiled in; dense-first always falls back to
/// lexical.
#[cfg(not(feature = "semantic-triage"))]
fn dense_vector_rows(
    _config: &Config,
    _repo_root: &std::path::Path,
    _query: &str,
) -> Option<Vec<Accumulator>> {
    None
}
