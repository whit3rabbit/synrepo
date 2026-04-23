//! Operational status snapshot shared between the CLI `status` command and the
//! runtime TUI dashboard. Computes read-only data; rendering is caller-side.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use time::OffsetDateTime;

use crate::{
    config::Config,
    core::ids::NodeId,
    pipeline::{
        compact::load_last_compaction_timestamp,
        context_metrics::{load as load_context_metrics, ContextMetrics},
        diagnostics::{collect_diagnostics, RuntimeDiagnostics},
        explain::{
            accounting::{load_totals, ExplainTotals},
            describe_active_provider, ExplainStatus,
        },
        export::load_manifest,
        recent_activity::{read_recent_activity, ActivityEntry, RecentActivityQuery},
        repair::{read_repair_log_degraded_marker, resolve_commentary_node, RepairLogDegraded},
        watch::load_reconcile_state,
    },
    store::{
        overlay::SqliteOverlayStore,
        sqlite::{PersistedGraphStats, SqliteGraphStore},
    },
    structure::graph::{snapshot, with_graph_read_snapshot, GraphReader},
};

/// Options controlling how the snapshot is built.
#[derive(Clone, Copy, Debug, Default)]
pub struct StatusOptions {
    /// Include recent activity entries (up to 20) in the snapshot.
    pub recent: bool,
    /// Compute commentary freshness (O(rows * graph_lookup)); default off.
    pub full: bool,
}

/// Sticky-marker state for the repair audit log.
#[derive(Clone, Debug)]
pub enum RepairAuditState {
    /// No marker present, or the last write succeeded.
    Ok,
    /// A prior repair-log append failed and the marker has not been cleared.
    Unavailable {
        /// RFC 3339 timestamp of the most recent failure.
        last_failure_at: String,
        /// Short reason captured when the marker was written.
        last_failure_reason: String,
    },
}

/// Display-ready summary of explain state. Carries both the resolved
/// provider identity (what would run if enabled) and the enablement status
/// (whether it actually will run, and why not when it won't).
#[derive(Clone, Debug)]
pub struct ExplainDisplay {
    /// Provider name (e.g. `anthropic`, `local`).
    pub provider: String,
    /// Default model for the provider, if one exists.
    pub model: Option<String>,
    /// Active local endpoint, if using ProviderKind::Local.
    pub local_endpoint: Option<String>,
    /// Source of the local endpoint.
    pub endpoint_source: crate::pipeline::explain::providers::EndpointSource,
    /// Resolved enablement status.
    pub status: ExplainStatus,
}

/// Commentary-coverage summary. `total` is always present when the overlay was
/// readable; `fresh` is only populated when the full-freshness scan ran.
#[derive(Clone, Debug)]
pub struct CommentaryCoverage {
    /// Number of commentary rows across all nodes. `None` when the overlay is
    /// not initialized or was unreadable.
    pub total: Option<usize>,
    /// Number of commentary rows whose stored hash matches the current node
    /// content hash. `None` unless the full scan was requested.
    pub fresh: Option<usize>,
    /// Human-readable one-line summary suitable for status text output.
    pub display: String,
}

/// Status of the published in-memory graph snapshot.
#[derive(Clone, Debug)]
pub struct GraphSnapshotStatus {
    /// Monotonic epoch of the published snapshot, or `0` when none is live.
    pub epoch: u64,
    /// Age in milliseconds since the snapshot was published.
    pub age_ms: u64,
    /// Approximate heap footprint of the published snapshot.
    pub size_bytes: usize,
    /// Active file count in the snapshot.
    pub file_count: usize,
    /// Active symbol count in the snapshot.
    pub symbol_count: usize,
    /// Active edge count in the snapshot.
    pub edge_count: usize,
}

/// Shared overlay handle opened once per snapshot build.
pub enum OverlayHandle {
    /// `.synrepo/overlay/` is absent.
    NotInitialized,
    /// Overlay directory exists but could not be opened.
    Unavailable(String),
    /// Overlay store open for read.
    Open(SqliteOverlayStore),
}

/// Full operational status snapshot. Captures everything the CLI `status`
/// command or the runtime dashboard needs to render without re-querying.
#[derive(Clone, Debug)]
pub struct StatusSnapshot {
    /// True when `.synrepo/config.toml` was present and parseable.
    pub initialized: bool,
    /// Loaded config, when initialized.
    pub config: Option<Config>,
    /// Runtime diagnostics (reconcile, watch, writer, embedding).
    pub diagnostics: Option<RuntimeDiagnostics>,
    /// Persisted graph counts; `None` when the graph store is missing.
    pub graph_stats: Option<PersistedGraphStats>,
    /// Published in-memory graph snapshot status.
    pub graph_snapshot: GraphSnapshotStatus,
    /// Export freshness summary line.
    pub export_freshness: String,
    /// Overlay LLM-cost summary line.
    pub overlay_cost_summary: String,
    /// Commentary coverage.
    pub commentary_coverage: CommentaryCoverage,
    /// Explain provider information, including enablement status and
    /// whether a provider API key was detected in the environment.
    pub explain_provider: Option<ExplainDisplay>,
    /// Per-repo explain accounting totals loaded from
    /// `.synrepo/state/explain-totals.json`. `None` when the file is
    /// missing, unreadable, or the repo is uninitialized.
    pub explain_totals: Option<ExplainTotals>,
    /// Context-serving metrics loaded from `.synrepo/state/context-metrics.json`.
    pub context_metrics: Option<ContextMetrics>,
    /// Last compaction timestamp, if any.
    pub last_compaction: Option<OffsetDateTime>,
    /// Repair audit state.
    pub repair_audit: RepairAuditState,
    /// Recent activity entries when `StatusOptions::recent` was set.
    pub recent_activity: Option<Vec<ActivityEntry>>,
    /// Resolved `.synrepo/` directory.
    pub synrepo_dir: PathBuf,
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
        explain_provider,
        explain_totals,
        context_metrics,
        last_compaction,
        repair_audit,
        recent_activity,
        synrepo_dir,
    }
}

fn current_graph_snapshot_status() -> GraphSnapshotStatus {
    let graph = snapshot::current();
    let age_ms = graph
        .published_at
        .duration_since(SystemTime::UNIX_EPOCH)
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
