//! Helper functions for building status snapshots.

use std::path::Path;
use std::str::FromStr;

use crate::{
    config::Config,
    core::ids::NodeId,
    pipeline::{
        compact::load_last_compaction_timestamp,
        context_metrics::load as load_context_metrics,
        diagnostics::collect_diagnostics,
        explain::{accounting::load_totals, describe_active_provider},
        export::load_manifest,
        recent_activity::{read_recent_activity, RecentActivityQuery},
        repair::{read_repair_log_degraded_marker, resolve_commentary_node, RepairLogDegraded},
        watch::load_reconcile_state,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::{snapshot, with_graph_read_snapshot, GraphReader},
};

use super::{
    CommentaryCoverage, ExplainDisplay, GraphSnapshotStatus, OverlayHandle, RepairAuditState,
    StatusOptions, StatusSnapshot,
};

pub(super) fn current_graph_snapshot_status() -> GraphSnapshotStatus {
    let graph = snapshot::current();
    let age_ms = graph
        .published_at
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|_| graph.published_at.elapsed().ok())
        .map(|elapsed| elapsed.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);

    GraphSnapshotStatus {
        epoch: graph.snapshot_epoch,
        age_ms,
        size_bytes: graph.approx_bytes(),
        file_count: graph.files.len(),
        symbol_count: graph.symbols.len(),
        edge_count: graph.all_edges().map(|edges| edges.len()).unwrap_or(0),
    }
}

/// Open the overlay store for status use.
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

/// Load the sticky repair-audit marker. Errors reading the marker are reported
/// as `Unavailable` so callers always get a usable state.
pub fn load_repair_audit_state(synrepo_dir: &Path) -> RepairAuditState {
    match read_repair_log_degraded_marker(synrepo_dir) {
        Ok(None) => RepairAuditState::Ok,
        Ok(Some(RepairLogDegraded {
            last_failure_at,
            last_failure_reason,
        })) => RepairAuditState::Unavailable {
            last_failure_at,
            last_failure_reason,
        },
        Err(e) => RepairAuditState::Unavailable {
            last_failure_at: String::new(),
            last_failure_reason: format!("marker read failed: {e}"),
        },
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
            Err(error) => return CommentaryCoverage::unavailable(error.to_string()),
        };
        return CommentaryCoverage::partial(total);
    }

    commentary_coverage_full(synrepo_dir, overlay)
}

fn commentary_coverage_full(
    synrepo_dir: &Path,
    overlay: &SqliteOverlayStore,
) -> CommentaryCoverage {
    let rows = match overlay.commentary_hashes() {
        Ok(rows) => rows,
        Err(error) => return CommentaryCoverage::unavailable(error.to_string()),
    };
    if rows.is_empty() {
        return CommentaryCoverage::full(0, 0);
    }
    let total = rows.len();

    let graph = match SqliteGraphStore::open_existing(&synrepo_dir.join("graph")) {
        Ok(graph) => graph,
        Err(_) => return CommentaryCoverage::graph_unreadable(total),
    };

    let fresh = with_graph_read_snapshot(&graph, |graph| {
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

/// Describe export freshness for status output.
pub fn export_freshness_summary(repo_root: &Path, synrepo_dir: &Path, config: &Config) -> String {
    let manifest = load_manifest(repo_root, config);
    match manifest {
        None => "absent (run `synrepo export` to generate)".to_string(),
        Some(m) => {
            let current_epoch = load_reconcile_state(synrepo_dir)
                .map(|r| r.last_reconcile_at)
                .unwrap_or_default();
            if m.last_reconcile_at == current_epoch {
                format!("current ({}, {})", m.format.as_str(), m.budget)
            } else {
                format!(
                    "stale (generated at {}, current epoch {})",
                    m.last_reconcile_at, current_epoch
                )
            }
        }
    }
}

/// Build a full status snapshot for `repo_root`. Read-only; never takes the
/// writer lock and never mutates the store.
pub fn build_status_snapshot(repo_root: &Path, opts: StatusOptions) -> StatusSnapshot {
    let synrepo_dir = Config::synrepo_dir(repo_root);

    let config = match Config::load(repo_root) {
        Ok(c) => Some(c),
        Err(_) => {
            return StatusSnapshot {
                initialized: false,
                config: None,
                diagnostics: None,
                graph_stats: None,
                graph_snapshot: current_graph_snapshot_status(),
                export_freshness: String::new(),
                overlay_cost_summary: String::new(),
                commentary_coverage: CommentaryCoverage::unavailable("not initialized"),
                agent_note_counts: None,
                explain_provider: None,
                explain_totals: None,
                context_metrics: None,
                last_compaction: None,
                repair_audit: RepairAuditState::Ok,
                recent_activity: None,
                synrepo_dir,
            };
        }
    };
    let config_ref = config.as_ref().expect("initialized implies config loaded");

    let diagnostics = collect_diagnostics(&synrepo_dir, config_ref);
    let graph_stats = {
        let graph_dir = synrepo_dir.join("graph");
        SqliteGraphStore::open_existing(&graph_dir)
            .ok()
            .and_then(|store| {
                with_graph_read_snapshot(&store, |_graph| store.persisted_stats()).ok()
            })
    };

    let export_freshness = export_freshness_summary(repo_root, &synrepo_dir, config_ref);
    let graph_snapshot = current_graph_snapshot_status();
    let overlay = open_status_overlay(&synrepo_dir);
    let overlay_cost_summary = overlay_cost_summary(&overlay);
    let commentary_coverage = commentary_coverage(&synrepo_dir, opts.full, &overlay);
    let agent_note_counts = match &overlay {
        OverlayHandle::Open(store) => store.note_counts_impl().ok(),
        OverlayHandle::NotInitialized | OverlayHandle::Unavailable(_) => None,
    };
    let last_compaction = load_last_compaction_timestamp(&synrepo_dir);
    let repair_audit = load_repair_audit_state(&synrepo_dir);
    let active = describe_active_provider(config_ref);
    let explain_provider = Some(ExplainDisplay {
        provider: active.provider.to_string(),
        model: active.model,
        local_endpoint: active.local_endpoint,
        endpoint_source: active.endpoint_source,
        status: active.status,
    });
    let explain_totals = load_totals(&synrepo_dir).ok().flatten();
    let context_metrics = load_context_metrics(&synrepo_dir).ok();

    let recent_activity = if opts.recent {
        let query = RecentActivityQuery {
            kinds: None,
            limit: 20,
            since: None,
        };
        read_recent_activity(&synrepo_dir, repo_root, config_ref, query).ok()
    } else {
        None
    };

    StatusSnapshot {
        initialized: true,
        config,
        diagnostics: Some(diagnostics),
        graph_stats,
        graph_snapshot,
        export_freshness,
        overlay_cost_summary,
        commentary_coverage,
        agent_note_counts,
        explain_provider,
        explain_totals,
        context_metrics,
        last_compaction,
        repair_audit,
        recent_activity,
        synrepo_dir,
    }
}
