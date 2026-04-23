//! Search and enrich explaind commentary docs for MCP callers.

use std::str::FromStr;

use serde::Serialize;
use time::format_description::well_known::Rfc3339;

use crate::core::ids::NodeId;
use crate::overlay::{with_overlay_read_snapshot, OverlayStore};
use crate::store::overlay::{derive_freshness, SqliteOverlayStore};
use crate::structure::graph::GraphReader;

use super::corpus::{docs_root, parse_commentary_doc_header, repo_relative_doc_path};
use super::index::search_commentary_index;

/// Stable label recorded in `source_store` of every hit: commentary docs are
/// overlay output, never graph facts.
const SOURCE_STORE_OVERLAY: &str = "overlay";

/// Hard ceiling on how many index matches we over-fetch before enriching and
/// truncating to the caller's `limit`.
const MAX_OVERSCAN: usize = 200;

/// Enriched search hit returned by `synrepo_docs_search`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommentaryDocHit {
    /// Annotated node ID.
    pub node_id: String,
    /// Qualified symbol name.
    pub qualified_name: String,
    /// Repo-relative source file path.
    pub source_path: String,
    /// Repo-relative explaind-doc path under `.synrepo/`.
    pub path: String,
    /// 1-based line number of the match.
    pub line: u32,
    /// Matching line content.
    pub content: String,
    /// Current commentary freshness state.
    pub commentary_state: String,
    /// Commentary generation timestamp.
    pub generated_at: String,
    /// Commentary model identity.
    pub model_identity: String,
    /// Advisory source store label.
    pub source_store: String,
}

/// Search the commentary-doc corpus and enrich each hit with live graph and
/// overlay metadata where possible.
pub fn search_commentary_docs(
    synrepo_dir: &std::path::Path,
    graph: &dyn GraphReader,
    overlay: Option<&SqliteOverlayStore>,
    query: &str,
    limit: usize,
) -> crate::Result<Vec<CommentaryDocHit>> {
    let overscan = limit.saturating_mul(4).max(limit).clamp(20, MAX_OVERSCAN);
    let matches = search_commentary_index(synrepo_dir, query, overscan)?;
    match overlay {
        Some(overlay) => with_overlay_read_snapshot(overlay, |overlay| {
            enrich_hits(synrepo_dir, graph, Some(overlay), matches, limit)
        }),
        None => enrich_hits(synrepo_dir, graph, None, matches, limit),
    }
}

fn enrich_hits(
    synrepo_dir: &std::path::Path,
    graph: &dyn GraphReader,
    overlay: Option<&dyn OverlayStore>,
    matches: Vec<syntext::SearchMatch>,
    limit: usize,
) -> crate::Result<Vec<CommentaryDocHit>> {
    let docs_root = docs_root(synrepo_dir);
    let mut hits = Vec::new();

    for hit in matches {
        let absolute_doc_path = docs_root.join(&hit.path);
        let Some(mut header) = parse_commentary_doc_header(&absolute_doc_path)? else {
            continue;
        };
        let Ok(node_id) = NodeId::from_str(&header.node_id) else {
            continue;
        };

        if let NodeId::Symbol(symbol_id) = node_id {
            if let Some(symbol) = graph.get_symbol(symbol_id)? {
                header.qualified_name = symbol.qualified_name;
                if let Some(file) = graph.get_file(symbol.file_id)? {
                    header.source_path = file.path.clone();
                    if let Some(overlay) = overlay {
                        if let Some(entry) = overlay.commentary_for(node_id)? {
                            header.commentary_state = derive_freshness(&entry, &file.content_hash)
                                .as_str()
                                .to_string();
                            header.model_identity = entry.provenance.model_identity;
                            header.generated_at = entry
                                .provenance
                                .generated_at
                                .format(&Rfc3339)
                                .map_err(|err| {
                                    crate::Error::Other(anyhow::anyhow!(
                                        "invalid commentary timestamp: {err}"
                                    ))
                                })?;
                        }
                    }
                }
            }
        }

        let path = repo_relative_doc_path(node_id)
            .unwrap_or_else(|| std::path::PathBuf::from(".synrepo/explain-docs").join(&hit.path))
            .to_string_lossy()
            .into_owned();

        hits.push(CommentaryDocHit {
            node_id: header.node_id,
            qualified_name: header.qualified_name,
            source_path: header.source_path,
            path,
            line: hit.line_number,
            content: String::from_utf8_lossy(&hit.line_content)
                .trim_end()
                .to_string(),
            commentary_state: header.commentary_state,
            generated_at: header.generated_at,
            model_identity: header.model_identity,
            source_store: SOURCE_STORE_OVERLAY.to_string(),
        });
        if hits.len() >= limit {
            break;
        }
    }

    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{CommentaryEntry, CommentaryProvenance};
    use crate::pipeline::explain::docs::{reconcile_commentary_docs, sync_commentary_index};
    use crate::store::overlay::SqliteOverlayStore;
    use crate::surface::card::compiler::tests::fixtures::{
        fresh_symbol_fixture, make_overlay_store,
    };
    use time::OffsetDateTime;

    #[test]
    fn docs_search_returns_live_symbol_metadata() {
        let (repo, graph, sym_id) = fresh_symbol_fixture();
        let hash = graph
            .file_by_path("src/lib.rs")
            .unwrap()
            .unwrap()
            .content_hash;
        let overlay = make_overlay_store(&repo);
        overlay
            .lock()
            .insert_commentary(CommentaryEntry {
                node_id: NodeId::Symbol(sym_id),
                text: "needle text".to_string(),
                provenance: CommentaryProvenance {
                    source_content_hash: hash,
                    pass_id: "test".to_string(),
                    model_identity: "fixture".to_string(),
                    generated_at: OffsetDateTime::UNIX_EPOCH,
                },
            })
            .unwrap();

        let synrepo_dir = repo.path().join(".synrepo");
        let overlay_store =
            SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
        let touched =
            reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();
        sync_commentary_index(&synrepo_dir, &touched).unwrap();

        let hits = search_commentary_docs(
            &synrepo_dir,
            &graph,
            Some(&overlay_store),
            "needle text",
            10,
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node_id, NodeId::Symbol(sym_id).to_string());
        assert_eq!(hits[0].source_path, "src/lib.rs");
        assert_eq!(hits[0].commentary_state, "fresh");
    }
}
