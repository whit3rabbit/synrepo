//! Maintenance hooks for `.synrepo/` runtime stores.
//!
//! Maintenance behavior is derived from the existing `storage-compatibility`
//! contract rather than defining its own retention rules. This ensures that
//! cleanup and compaction decisions remain consistent with the store class and
//! compatibility policy already defined in `store::compatibility`.
//!
//! ## Design
//!
//! `plan_maintenance` queries the current compatibility state and maps each
//! store's `CompatAction` to a `MaintenanceAction`. Blocking actions
//! (`Block`, `MigrateRequired`) are surfaced as informational guidance but
//! never executed automatically; they require explicit operator intervention.
//!
//! `execute_maintenance` applies the plan: clearing or rebuilding stores
//! whose compatibility actions indicate stale or incompatible contents.
//!
//! ## Compact
//!
//! `plan_compact` queries overlay stats, repair-log age, and index freshness
//! to build a `CompactPlan`. `execute_compact` runs all retention actions
//! in sequence: commentary compaction, cross-link compaction, repair-log
//! rotation, WAL checkpoint, and optional index rebuild.

use std::path::Path;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{
    config::Config,
    store::compatibility::{self, clear_store_contents, CompatAction, StoreId},
};

/// Compute a retention cutoff timestamp (formatted as RFC3339 string).
/// Used by overlay compaction to compute deletion boundaries.
pub fn retention_cutoff(retention_days: u32) -> crate::Result<String> {
    let cutoff = OffsetDateTime::now_utc() - time::Duration::days(retention_days as i64);
    cutoff
        .format(&Rfc3339)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("format cutoff: {e}")))
}

/// Retention policy presets for compaction operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactPolicy {
    /// Conservative retention: 30-day commentary window, 90-day audit window.
    Default,
    /// Aggressive retention: 7-day commentary window, 30-day audit window.
    Aggressive,
    /// Audit-heavy retention: 60-day commentary window, 180-day audit window.
    AuditHeavy,
}

impl CompactPolicy {
    /// Stable string identifier for serialization and logging.
    pub fn as_str(self) -> &'static str {
        match self {
            CompactPolicy::Default => "default",
            CompactPolicy::Aggressive => "aggressive",
            CompactPolicy::AuditHeavy => "audit_heavy",
        }
    }

    /// Days to retain stale commentary entries before compaction.
    pub fn commentary_retention_days(&self) -> u32 {
        match self {
            CompactPolicy::Default => 30,
            CompactPolicy::Aggressive => 7,
            CompactPolicy::AuditHeavy => 60,
        }
    }

    /// Days to retain promoted/rejected cross-link audit rows before summarization.
    pub fn audit_retention_days(&self) -> u32 {
        match self {
            CompactPolicy::Default => 90,
            CompactPolicy::Aggressive => 30,
            CompactPolicy::AuditHeavy => 180,
        }
    }

    /// Days to retain repair-log entries before summarization or truncation.
    pub fn repair_log_retention_days(&self) -> u32 {
        match self {
            CompactPolicy::Default => 30,
            CompactPolicy::Aggressive => 7,
            CompactPolicy::AuditHeavy => 60,
        }
    }
}

impl Default for CompactPolicy {
    fn default() -> Self {
        CompactPolicy::Default
    }
}

/// Action to perform during a compaction pass.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactAction {
    /// No action needed for this component.
    Skip,
    /// Compact stale commentary entries.
    CompactCommentary,
    /// Compact cross-link audit rows.
    CompactCrossLinks,
    /// Rotate the repair-log file (summarize old entries).
    RotateRepairLog,
    /// Run WAL checkpoint on both graph and overlay databases.
    WalCheckpoint,
    /// Rebuild the lexical index store.
    RebuildIndex,
}

impl CompactAction {
    /// Stable string identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            CompactAction::Skip => "skip",
            CompactAction::CompactCommentary => "compact_commentary",
            CompactAction::CompactCrossLinks => "compact_cross_links",
            CompactAction::RotateRepairLog => "rotate_repair_log",
            CompactAction::WalCheckpoint => "wal_checkpoint",
            CompactAction::RebuildIndex => "rebuild_index",
        }
    }
}

/// Compactable statistics for a specific component.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CompactStats {
    /// Number of compactable commentary entries (stale and older than retention).
    pub compactable_commentary: usize,
    /// Number of compactable cross-link audit rows (promoted/rejected older than retention).
    pub compactable_cross_links: usize,
    /// Number of repair-log entries beyond the retention window.
    pub repair_log_entries_beyond_window: usize,
    /// Last compaction timestamp (None if never compacted).
    pub last_compaction_timestamp: Option<OffsetDateTime>,
}

/// Compact plan derived from querying current store state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CompactPlan {
    /// Per-component actions to perform.
    pub actions: Vec<ComponentCompact>,
    /// Estimated stats based on current state.
    pub estimated_stats: CompactStats,
}

/// A single component's planned compact action.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentCompact {
    /// The component this action applies to.
    pub component: CompactComponent,
    /// The action to perform.
    pub action: CompactAction,
    /// Human-readable reason or description.
    pub reason: String,
}

/// Components that can be compacted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactComponent {
    Commentary,
    CrossLinks,
    RepairLog,
    Wal,
    Index,
}

impl CompactComponent {
    /// Stable string identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            CompactComponent::Commentary => "commentary",
            CompactComponent::CrossLinks => "cross_links",
            CompactComponent::RepairLog => "repair_log",
            CompactComponent::Wal => "wal",
            CompactComponent::Index => "index",
        }
    }
}

/// Summary of a compaction pass.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactSummary {
    /// Number of commentary entries compacted.
    pub commentary_compacted: usize,
    /// Number of cross-link audit rows compacted (or summarized).
    pub cross_links_compacted: usize,
    /// Number of repair-log entries summarized.
    pub repair_log_summarized: usize,
    /// Whether WAL checkpoint succeeded.
    pub wal_checkpoint_completed: bool,
    /// Whether index was rebuilt.
    pub index_rebuilt: bool,
    /// Timestamp of this compaction pass.
    pub compaction_timestamp: OffsetDateTime,
}

impl CompactSummary {
    /// Create a default (empty) compaction summary.
    pub fn default() -> Self {
        Self {
            commentary_compacted: 0,
            cross_links_compacted: 0,
            repair_log_summarized: 0,
            wal_checkpoint_completed: false,
            index_rebuilt: false,
            compaction_timestamp: OffsetDateTime::now_utc(),
        }
    }

    /// Render a brief human-readable summary.
    pub fn render(&self) -> String {
        let mut parts = Vec::new();
        if self.commentary_compacted > 0 {
            parts.push(format!("{} commentary entries compacted", self.commentary_compacted));
        }
        if self.cross_links_compacted > 0 {
            parts.push(format!("{} cross-link rows compacted", self.cross_links_compacted));
        }
        if self.repair_log_summarized > 0 {
            parts.push(format!("{} repair-log entries summarized", self.repair_log_summarized));
        }
        if self.wal_checkpoint_completed {
            parts.push("WAL checkpoint completed".to_string());
        }
        if self.index_rebuilt {
            parts.push("Index rebuilt".to_string());
        }

        if parts.is_empty() {
            "No compaction work needed.".to_string()
        } else {
            parts.join("; ")
        }
    }
}

/// Action to apply to a specific store during a maintenance pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceAction {
    /// Store is healthy; no action needed.
    Skip,
    /// Store contents should be cleared (invalidated).
    Clear,
    /// Store contents should be cleared and queued for rebuild on next init.
    Rebuild,
}

impl MaintenanceAction {
    /// Stable string identifier for this action.
    pub fn as_str(self) -> &'static str {
        match self {
            MaintenanceAction::Skip => "skip",
            MaintenanceAction::Clear => "clear",
            MaintenanceAction::Rebuild => "rebuild",
        }
    }
}

/// Planned maintenance action for a single store.
#[derive(Clone, Debug)]
pub struct StoreMaintenance {
    /// Store this action applies to.
    pub store_id: StoreId,
    /// Action to apply.
    pub action: MaintenanceAction,
    /// Human-readable reason derived from the compatibility evaluation.
    pub reason: String,
}

/// Maintenance plan derived from the current compatibility state.
#[derive(Clone, Debug)]
pub struct MaintenancePlan {
    /// Per-store maintenance actions.
    pub actions: Vec<StoreMaintenance>,
}

impl MaintenancePlan {
    /// Returns true when at least one store requires non-trivial maintenance.
    pub fn has_work(&self) -> bool {
        self.actions
            .iter()
            .any(|a| a.action != MaintenanceAction::Skip)
    }

    /// Iterator over only the non-trivial (non-skip) actions.
    pub fn pending_actions(&self) -> impl Iterator<Item = &StoreMaintenance> {
        self.actions
            .iter()
            .filter(|a| a.action != MaintenanceAction::Skip)
    }
}

/// Summary of maintenance executed in one pass.
#[derive(Clone, Debug, Default)]
pub struct MaintenanceSummary {
    /// Number of stores whose contents were cleared or rebuilt.
    pub stores_cleared: usize,
    /// Number of stores that were already healthy and skipped.
    pub stores_skipped: usize,
}

impl MaintenanceSummary {
    /// Render a brief human-readable summary.
    pub fn render(&self) -> String {
        if self.stores_cleared == 0 {
            "No maintenance needed; all stores are healthy.".to_string()
        } else {
            format!(
                "Cleared {} store(s); {} store(s) already healthy.",
                self.stores_cleared, self.stores_skipped,
            )
        }
    }
}

/// Plan maintenance actions by consulting the current compatibility state.
///
/// Derives maintenance needs from the `storage-compatibility` contract so
/// that cleanup decisions stay consistent with the declared store classes
/// and compatibility policies already established in `storage-compatibility-v1`.
pub fn plan_maintenance(synrepo_dir: &Path, config: &Config) -> crate::Result<MaintenancePlan> {
    let runtime_exists = synrepo_dir.exists();
    let report = compatibility::evaluate_runtime(synrepo_dir, runtime_exists, config)?;

    let actions = report
        .entries
        .iter()
        .map(|entry| StoreMaintenance {
            store_id: entry.store_id,
            action: compat_action_to_maintenance(entry.action),
            reason: entry.reason.clone(),
        })
        .collect();

    Ok(MaintenancePlan { actions })
}

/// Execute the maintenance plan, applying non-blocking actions to disk.
///
/// Blocking actions (`Block`, `MigrateRequired`) are not executed here and
/// do not contribute to `stores_cleared`; they require explicit operator
/// intervention such as removing or migrating the affected store before
/// re-running `synrepo init`.
pub fn execute_maintenance(
    synrepo_dir: &Path,
    plan: &MaintenancePlan,
) -> crate::Result<MaintenanceSummary> {
    let mut summary = MaintenanceSummary::default();

    for store_action in plan.pending_actions() {
        clear_store_contents(synrepo_dir, store_action.store_id)?;
        summary.stores_cleared += 1;
        tracing::info!(
            store = %store_action.store_id.as_str(),
            action = %store_action.action.as_str(),
            reason = %store_action.reason,
            "maintenance action applied"
        );
    }
    summary.stores_skipped = plan.actions.len() - summary.stores_cleared;

    Ok(summary)
}

/// Map a compatibility action to the corresponding maintenance action.
fn compat_action_to_maintenance(action: CompatAction) -> MaintenanceAction {
    match action {
        CompatAction::Continue => MaintenanceAction::Skip,
        CompatAction::Rebuild | CompatAction::ClearAndRecreate => MaintenanceAction::Rebuild,
        CompatAction::Invalidate => MaintenanceAction::Clear,
        // Blocking actions require explicit operator intervention and are
        // intentionally not executed by the maintenance runner.
        CompatAction::MigrateRequired | CompatAction::Block => MaintenanceAction::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::compatibility::{ensure_runtime_layout, write_runtime_snapshot};
    use tempfile::tempdir;

    fn init_runtime(synrepo_dir: &Path) {
        ensure_runtime_layout(synrepo_dir).unwrap();
        write_runtime_snapshot(synrepo_dir, &Config::default()).unwrap();
    }

    #[test]
    fn plan_has_no_work_on_freshly_initialized_runtime() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        let plan = plan_maintenance(&synrepo_dir, &Config::default()).unwrap();
        assert!(
            !plan.has_work(),
            "fresh runtime with current snapshot should need no maintenance"
        );
    }

    #[test]
    fn plan_schedules_rebuild_for_stale_index() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        // Materialize the index so the compatibility check sees it.
        std::fs::write(synrepo_dir.join("index/manifest.json"), "{}").unwrap();

        // Change an index-sensitive config field to trigger a Rebuild action.
        let updated = Config {
            roots: vec!["src".to_string()],
            ..Config::default()
        };
        let plan = plan_maintenance(&synrepo_dir, &updated).unwrap();

        let index_action = plan
            .actions
            .iter()
            .find(|a| a.store_id == StoreId::Index)
            .expect("index store must be in plan");
        assert_eq!(index_action.action, MaintenanceAction::Rebuild);
        assert!(plan.has_work());
    }

    #[test]
    fn execute_maintenance_clears_stale_index_contents() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        let index_file = synrepo_dir.join("index/manifest.json");
        std::fs::write(&index_file, "{}").unwrap();
        assert!(
            index_file.exists(),
            "index file must exist before maintenance"
        );

        let updated = Config {
            roots: vec!["src".to_string()],
            ..Config::default()
        };
        let plan = plan_maintenance(&synrepo_dir, &updated).unwrap();
        assert!(plan.has_work());

        let summary = execute_maintenance(&synrepo_dir, &plan).unwrap();
        assert!(summary.stores_cleared >= 1);

        let remaining: Vec<_> = std::fs::read_dir(synrepo_dir.join("index"))
            .unwrap()
            .collect();
        assert!(
            remaining.is_empty(),
            "index must be empty after maintenance"
        );
    }

    #[test]
    fn execute_maintenance_skips_healthy_stores() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        let plan = plan_maintenance(&synrepo_dir, &Config::default()).unwrap();
        let summary = execute_maintenance(&synrepo_dir, &plan).unwrap();

        assert_eq!(summary.stores_cleared, 0);
        // All known stores are covered.
        assert_eq!(
            summary.stores_skipped,
            StoreId::ALL.len(),
            "all stores should be skipped when none need maintenance"
        );
    }

    #[test]
    fn maintenance_summary_render_reflects_cleared_count() {
        let summary = MaintenanceSummary {
            stores_cleared: 2,
            stores_skipped: 4,
        };
        let rendered = summary.render();
        assert!(rendered.contains("Cleared 2"), "must mention cleared count");
        assert!(rendered.contains('4'), "must mention skipped count");
    }

    #[test]
    fn maintenance_summary_render_for_no_work_done() {
        let summary = MaintenanceSummary {
            stores_cleared: 0,
            stores_skipped: 6,
        };
        assert!(summary.render().contains("No maintenance needed"));
    }
}
