//! Overlay and commentary coverage for status.

use std::path::Path;
use std::str::FromStr;

use synrepo::{
    core::ids::NodeId,
    pipeline::repair::resolve_commentary_node,
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

/// Shared overlay handle for status consumers, opened once per invocation.
pub enum OverlayHandle {
    NotInitialized,
    Unavailable(String),
    Open(SqliteOverlayStore),
}

pub struct CommentaryCoverage {
    pub total: Option<usize>,
    pub fresh: Option<usize>,
    pub display: String,
}

impl CommentaryCoverage {
    fn not_initialized() -> Self {
        Self {
            total: None,
            fresh: None,
            display: "not initialized".to_string(),
        }
    }

    fn unavailable(reason: impl std::fmt::Display) -> Self {
        Self {
            total: None,
            fresh: None,
            display: format!("unavailable ({reason})"),
        }
    }

    fn partial(total: usize) -> Self {
        let display = if total == 0 {
            "0 entries".to_string()
        } else {
            format!("{total} entries (run `synrepo status --full` for freshness)")
        };
        Self {
            total: Some(total),
            fresh: None,
            display,
        }
    }

    fn full(total: usize, fresh: usize) -> Self {
        Self {
            total: Some(total),
            fresh: Some(fresh),
            display: format!("{fresh} fresh / {total} total nodes with commentary"),
        }
    }

    fn graph_unreadable(total: usize) -> Self {
        Self {
            total: Some(total),
            fresh: None,
            display: format!("{total} entries (graph unreadable)"),
        }
    }
}

pub fn open_status_overlay(synrepo_dir: &Path) -> OverlayHandle {
    let overlay_dir = synrepo_dir.join("overlay");
    if !SqliteOverlayStore::db_path(&overlay_dir).exists() {
        return OverlayHandle::NotInitialized;
    }
    match SqliteOverlayStore::open_existing(&overlay_dir) {
        Ok(store) => OverlayHandle::Open(store),
        Err(e) => OverlayHandle::Unavailable(e.to_string()),
    }
}

/// Describe overlay cost for status output. Scans on demand; no caching.
pub fn overlay_cost_summary(overlay: &OverlayHandle) -> String {
    let overlay = match overlay {
        OverlayHandle::NotInitialized => return "no overlay (0 LLM calls)".to_string(),
        OverlayHandle::Unavailable(e) => return format!("unavailable ({e})"),
        OverlayHandle::Open(store) => store,
    };

    let cross_link_gens = match overlay.cross_link_generation_count() {
        Ok(n) => n,
        Err(e) => return format!("unavailable (cross-link count query failed: {e})"),
    };
    let commentary_entries = match overlay.commentary_count() {
        Ok(n) => n,
        Err(e) => return format!("unavailable (commentary count query failed: {e})"),
    };
    let total_calls = cross_link_gens + commentary_entries;
    let pending_promotion = match overlay.cross_link_state_counts() {
        Ok(counts) => counts.pending_promotion,
        Err(e) => return format!("unavailable (cross-link state count query failed: {e})"),
    };

    format!(
        "{total_calls} LLM calls ({cross_link_gens} cross-link gen, {commentary_entries} commentary){pending_promotion_str}",
        pending_promotion_str = if pending_promotion > 0 {
            format!(", {pending_promotion} pending promotion")
        } else {
            String::new()
        }
    )
}

/// Summarize commentary coverage. When `full` is false, avoids opening the
/// graph store and reading every commentary row; returns only the row count.
pub fn commentary_coverage(
    synrepo_dir: &Path,
    full: bool,
    overlay: &OverlayHandle,
) -> CommentaryCoverage {
    let overlay = match overlay {
        OverlayHandle::NotInitialized => return CommentaryCoverage::not_initialized(),
        OverlayHandle::Unavailable(e) => return CommentaryCoverage::unavailable(e),
        OverlayHandle::Open(store) => store,
    };

    if !full {
        let total = match overlay.commentary_count() {
            Ok(n) => n,
            Err(error) => return CommentaryCoverage::unavailable(&error),
        };
        return CommentaryCoverage::partial(total);
    }

    commentary_coverage_full(synrepo_dir, overlay)
}

/// Full freshness scan: walks every commentary row through a graph read
/// snapshot and compares stored hashes against current content hashes.
fn commentary_coverage_full(
    synrepo_dir: &Path,
    overlay: &SqliteOverlayStore,
) -> CommentaryCoverage {
    let rows = match overlay.commentary_hashes() {
        Ok(rows) => rows,
        Err(error) => return CommentaryCoverage::unavailable(&error),
    };
    if rows.is_empty() {
        return CommentaryCoverage::full(0, 0);
    }
    let total = rows.len();

    let graph = match SqliteGraphStore::open_existing(&synrepo_dir.join("graph")) {
        Ok(graph) => graph,
        Err(_) => return CommentaryCoverage::graph_unreadable(total),
    };

    let fresh = synrepo::structure::graph::with_graph_read_snapshot(&graph, |graph| {
        let mut fresh = 0usize;
        for (node_id_str, stored_hash) in &rows {
            let Ok(node_id) = NodeId::from_str(node_id_str) else {
                continue;
            };
            if resolve_commentary_node(graph, node_id)
                .ok()
                .flatten()
                .is_some_and(|snap| &snap.content_hash == stored_hash)
            {
                fresh += 1;
            }
        }
        Ok(fresh)
    })
    .unwrap_or(0);

    CommentaryCoverage::full(total, fresh)
}
