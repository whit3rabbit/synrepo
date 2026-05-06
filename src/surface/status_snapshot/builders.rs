//! Helper functions for building status snapshots.

use std::{path::Path, str::FromStr};

use crate::{
    config::Config,
    core::ids::NodeId,
    overlay::with_overlay_read_snapshot,
    pipeline::{
        compact::load_last_compaction_timestamp,
        context_metrics::load_optional as load_context_metrics,
        diagnostics::collect_diagnostics,
        explain::{accounting::load_totals, describe_active_provider},
        recent_activity::{read_recent_activity, RecentActivityQuery},
        repair::{read_repair_log_degraded_marker, resolve_commentary_node, RepairLogDegraded},
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::{snapshot, with_graph_read_snapshot, GraphReader},
};

use super::{
    build_export_status, CommentaryCoverage, ExplainDisplay, ExportState, ExportStatus,
    GraphSnapshotStatus, OverlayHandle, RepairAuditState, StatusOptions, StatusSnapshot,
};

pub(super) fn current_graph_snapshot_status(repo_root: &Path) -> GraphSnapshotStatus {
    let Some(graph) = snapshot::current(repo_root) else {
        return GraphSnapshotStatus {
            epoch: 0,
            age_ms: 0,
            size_bytes: 0,
            file_count: 0,
            symbol_count: 0,
            edge_count: 0,
        };
    };
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

    let (cross_link_gens, commentary_entries, pending_promotion) =
        match with_overlay_read_snapshot(overlay, |overlay| {
            let cross_link_gens = overlay.cross_link_generation_count()?;
            let commentary_entries = overlay.commentary_count()?;
            let pending_promotion = overlay.cross_link_state_counts()?.pending_promotion;
            Ok((cross_link_gens, commentary_entries, pending_promotion))
        }) {
            Ok(summary) => summary,
            Err(e) => return format!("unavailable (overlay cost query failed: {e})"),
        };
    let total_calls = cross_link_gens + commentary_entries;

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
        let total = match with_overlay_read_snapshot(overlay, |overlay| overlay.commentary_count())
        {
            Ok(n) => n,
            Err(error) => return CommentaryCoverage::unavailable(error.to_string()),
        };
        let (estimated_fresh, estimated_stale_ratio, estimate_confidence) =
            estimate_commentary_freshness(synrepo_dir, overlay, total);
        return CommentaryCoverage::partial(
            total,
            estimated_fresh,
            estimated_stale_ratio,
            estimate_confidence,
        );
    }

    commentary_coverage_full(synrepo_dir, overlay)
}

fn commentary_coverage_full(
    synrepo_dir: &Path,
    overlay: &SqliteOverlayStore,
) -> CommentaryCoverage {
    let graph = match SqliteGraphStore::open_existing(&synrepo_dir.join("graph")) {
        Ok(graph) => graph,
        Err(_) => {
            let total = with_overlay_read_snapshot(overlay, |overlay| overlay.commentary_count())
                .unwrap_or(0);
            return CommentaryCoverage::graph_unreadable(total);
        }
    };

    let (total, fresh) = match with_overlay_read_snapshot(overlay, |overlay| {
        with_graph_read_snapshot(&graph, |graph| {
            let mut fresh = 0usize;
            let total = overlay.scan_commentary_hashes(|node_id, source_hash| {
                let Ok(node_id) = NodeId::from_str(node_id) else {
                    return Ok(());
                };
                if resolve_commentary_node(graph, node_id)?
                    .is_some_and(|snap| source_hash == snap.content_hash)
                {
                    fresh += 1;
                }
                Ok(())
            })?;
            Ok((total, fresh))
        })
    }) {
        Ok(counts) => counts,
        Err(error) => return CommentaryCoverage::unavailable(error.to_string()),
    };

    CommentaryCoverage::full(total, fresh)
}

fn estimate_commentary_freshness(
    synrepo_dir: &Path,
    overlay: &SqliteOverlayStore,
    total: usize,
) -> (Option<usize>, Option<f32>, Option<String>) {
    if total == 0 {
        return (Some(0), Some(0.0), Some("high".to_string()));
    }

    let age_ratio =
        with_overlay_read_snapshot(overlay, |overlay| overlay.commentary_generated_at_bounds())
            .ok()
            .flatten()
            .map(|(oldest, _)| {
                let age = time::OffsetDateTime::now_utc() - oldest;
                let days = age.whole_days();
                if days < 14 {
                    0.0_f32
                } else if days < 30 {
                    0.2_f32
                } else {
                    0.5_f32
                }
            });

    let drift_ratio = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))
        .ok()
        .and_then(|graph| graph.latest_drift_average().ok().flatten())
        .map(|score| score.clamp(0.0, 1.0));

    let stale_ratio = match (age_ratio, drift_ratio) {
        (Some(age), Some(drift)) => age.max(drift),
        (Some(age), None) => age,
        (None, Some(drift)) => drift,
        (None, None) => return (None, None, None),
    };
    let estimated_fresh = ((total as f32) * (1.0 - stale_ratio)).round() as usize;
    let confidence = if age_ratio.is_some() && drift_ratio.is_some() {
        "medium"
    } else {
        "low"
    };
    (
        Some(estimated_fresh.min(total)),
        Some(stale_ratio),
        Some(confidence.to_string()),
    )
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
                graph_snapshot: current_graph_snapshot_status(repo_root),
                export_freshness: String::new(),
                export_status: ExportStatus {
                    state: ExportState::Absent,
                    display: String::new(),
                    export_dir: String::new(),
                    format: None,
                    budget: None,
                },
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

    let export_status = build_export_status(repo_root, &synrepo_dir, config_ref);
    let export_freshness = export_status.display.clone();
    let graph_snapshot = current_graph_snapshot_status(repo_root);
    let overlay = open_status_overlay(&synrepo_dir);
    let overlay_cost_summary = overlay_cost_summary(&overlay);
    let commentary_coverage = commentary_coverage(&synrepo_dir, opts.full, &overlay);
    let agent_note_counts = match &overlay {
        OverlayHandle::Open(store) => {
            with_overlay_read_snapshot(store, |store| store.note_counts_impl()).ok()
        }
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
    let context_metrics = load_context_metrics(&synrepo_dir).ok().flatten();

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
        export_status,
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
