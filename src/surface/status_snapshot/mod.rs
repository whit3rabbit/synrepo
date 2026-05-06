//! Operational status snapshot shared between the CLI `status` command and the
//! runtime TUI dashboard. Computes read-only data; rendering is caller-side.

use std::path::PathBuf;

use time::OffsetDateTime;

use crate::{
    config::Config,
    pipeline::{
        context_metrics::ContextMetrics,
        diagnostics::RuntimeDiagnostics,
        explain::{accounting::ExplainTotals, providers::EndpointSource, ExplainStatus},
        recent_activity::ActivityEntry,
    },
    store::sqlite::PersistedGraphStats,
};

mod builders;
mod export_status;
pub use builders::*;
pub use export_status::*;

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
    pub endpoint_source: EndpointSource,
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
    /// Estimated fresh commentary rows from cheap aggregate signals.
    pub estimated_fresh: Option<usize>,
    /// Estimated stale ratio from aggregate drift/commentary age signals.
    pub estimated_stale_ratio: Option<f32>,
    /// Confidence label for the estimate.
    pub estimate_confidence: Option<String>,
    /// Human-readable one-line summary suitable for status text output.
    pub display: String,
}

impl CommentaryCoverage {
    pub(super) fn not_initialized() -> Self {
        // `overlay.db` is created lazily on the first commentary or cross-link
        // write. An empty overlay after `synrepo init` is the expected
        // baseline; `not initialized` would imply a setup error.
        Self {
            total: None,
            fresh: None,
            estimated_fresh: None,
            estimated_stale_ratio: None,
            estimate_confidence: None,
            display: "no overlay writes yet".to_string(),
        }
    }

    pub(super) fn unavailable(reason: impl std::fmt::Display) -> Self {
        Self {
            total: None,
            fresh: None,
            estimated_fresh: None,
            estimated_stale_ratio: None,
            estimate_confidence: None,
            display: format!("unavailable ({reason})"),
        }
    }

    pub(super) fn partial(
        total: usize,
        estimated_fresh: Option<usize>,
        estimated_stale_ratio: Option<f32>,
        estimate_confidence: Option<String>,
    ) -> Self {
        let display = if total == 0 {
            "0 entries".to_string()
        } else if let (Some(fresh), Some(confidence)) =
            (estimated_fresh, estimate_confidence.as_deref())
        {
            format!("{total} entries (~{fresh} estimated fresh, {confidence} confidence)")
        } else {
            format!("{total} entries (run `synrepo status --full` for freshness)")
        };
        Self {
            total: Some(total),
            fresh: None,
            estimated_fresh,
            estimated_stale_ratio,
            estimate_confidence,
            display,
        }
    }

    pub(super) fn full(total: usize, fresh: usize) -> Self {
        Self {
            total: Some(total),
            fresh: Some(fresh),
            estimated_fresh: Some(fresh),
            estimated_stale_ratio: Some(if total == 0 {
                0.0
            } else {
                1.0 - (fresh as f32 / total as f32)
            }),
            estimate_confidence: Some("exact".to_string()),
            display: format!("{fresh} fresh / {total} total nodes with commentary"),
        }
    }

    pub(super) fn graph_unreadable(total: usize) -> Self {
        Self {
            total: Some(total),
            fresh: None,
            estimated_fresh: None,
            estimated_stale_ratio: None,
            estimate_confidence: None,
            display: format!("{total} entries (graph unreadable)"),
        }
    }
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
    Open(crate::store::overlay::SqliteOverlayStore),
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
    /// Structured context export status.
    pub export_status: ExportStatus,
    /// Overlay LLM-cost summary line.
    pub overlay_cost_summary: String,
    /// Commentary coverage.
    pub commentary_coverage: CommentaryCoverage,
    /// Agent-note lifecycle counts when the overlay is readable.
    pub agent_note_counts: Option<crate::overlay::AgentNoteCounts>,
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
