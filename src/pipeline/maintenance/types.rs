//! Data types for maintenance and compaction operations.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::store::compatibility::StoreId;

/// Retention policy presets for compaction operations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactPolicy {
    /// Conservative retention: 30-day commentary window, 90-day audit window.
    #[default]
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
    /// Symbol commentary overlay entries (LLM-authored prose).
    Commentary,
    /// Cross-link overlay entries (prose-to-code associations).
    CrossLinks,
    /// Repair log JSONL file (`.synrepo/state/repair-log.jsonl`).
    RepairLog,
    /// Write-Ahead Log (WAL) files for SQLite stores.
    Wal,
    /// Syntext lexical index (`.synrepo/index/`).
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

impl Default for CompactSummary {
    fn default() -> Self {
        Self {
            commentary_compacted: 0,
            cross_links_compacted: 0,
            repair_log_summarized: 0,
            wal_checkpoint_completed: false,
            index_rebuilt: false,
            compaction_timestamp: OffsetDateTime::now_utc(),
        }
    }
}

impl CompactSummary {
    /// Render a brief human-readable summary.
    pub fn render(&self) -> String {
        let mut parts = Vec::new();
        if self.commentary_compacted > 0 {
            parts.push(format!(
                "{} commentary entries compacted",
                self.commentary_compacted
            ));
        }
        if self.cross_links_compacted > 0 {
            parts.push(format!(
                "{} cross-link rows compacted",
                self.cross_links_compacted
            ));
        }
        if self.repair_log_summarized > 0 {
            parts.push(format!(
                "{} repair-log entries summarized",
                self.repair_log_summarized
            ));
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
