use serde::{Deserialize, Serialize};

use super::stable::{DriftClass, RepairAction, RepairSurface, Severity};

/// One repair finding: a named surface, its drift class, severity, and recommended action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepairFinding {
    /// The named surface this finding applies to.
    pub surface: RepairSurface,
    /// How the surface has drifted.
    pub drift_class: DriftClass,
    /// Severity and response category.
    pub severity: Severity,
    /// Optional stable identifier of the affected target (node ID, path, etc.).
    pub target_id: Option<String>,
    /// What the repair loop should do about this finding.
    pub recommended_action: RepairAction,
    /// Human-readable explanation of the finding.
    pub notes: Option<String>,
}

/// Report produced by `synrepo check`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepairReport {
    /// RFC 3339 UTC timestamp of when the check ran.
    pub checked_at: String,
    /// One finding per inspected surface.
    pub findings: Vec<RepairFinding>,
}

/// Options for `synrepo sync`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncOptions {
    /// If true, generate new cross-link candidates for the whole repository.
    #[serde(default)]
    pub generate_cross_links: bool,
    /// If true, re-run generation for stale candidates.
    #[serde(default)]
    pub regenerate_cross_links: bool,
}

impl RepairReport {
    /// Returns true if any finding has `Severity::Actionable` and a non-None action.
    pub fn has_actionable(&self) -> bool {
        self.findings.iter().any(|f| {
            f.severity == Severity::Actionable && f.recommended_action != RepairAction::None
        })
    }

    /// Returns true if any finding has `Severity::Blocked`.
    pub fn has_blocked(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == Severity::Blocked)
    }

    /// Render a human-readable summary for CLI output.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("check: {}\n", self.checked_at));
        for finding in &self.findings {
            out.push_str(&format!(
                "  [{severity}] {surface}: {drift} — {action}\n",
                severity = finding.severity.as_str(),
                surface = finding.surface.as_str(),
                drift = finding.drift_class.as_str(),
                action = finding.recommended_action.as_str(),
            ));
            if let Some(notes) = &finding.notes {
                out.push_str(&format!("    {notes}\n"));
            }
        }
        out
    }
}

/// Summary of one `synrepo sync` execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncSummary {
    /// RFC 3339 UTC timestamp of when the sync ran.
    pub synced_at: String,
    /// Findings that were successfully auto-repaired.
    pub repaired: Vec<RepairFinding>,
    /// Findings that were noted but left untouched (report-only, blocked, or unsupported).
    pub report_only: Vec<RepairFinding>,
    /// Findings that could not be repaired due to blocking conditions.
    pub blocked: Vec<RepairFinding>,
}

impl SyncSummary {
    /// Render a human-readable summary for CLI output.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("sync: {}\n", self.synced_at));
        out.push_str(&format!("  repaired:    {}\n", self.repaired.len()));
        for f in &self.repaired {
            out.push_str(&format!("    [ok]      {}\n", f.surface.as_str()));
        }
        if !self.report_only.is_empty() {
            out.push_str(&format!(
                "  report-only: {} (human review needed or unsupported)\n",
                self.report_only.len()
            ));
            for f in &self.report_only {
                out.push_str(&format!(
                    "    [skip]    {} — {}\n",
                    f.surface.as_str(),
                    f.severity.as_str()
                ));
            }
        }
        if !self.blocked.is_empty() {
            out.push_str(&format!("  blocked:     {}\n", self.blocked.len()));
            for f in &self.blocked {
                out.push_str(&format!(
                    "    [blocked] {}: {}\n",
                    f.surface.as_str(),
                    f.notes.as_deref().unwrap_or("no details")
                ));
            }
        }
        out
    }
}

/// Outcome of one `synrepo sync` execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    /// All actionable findings were repaired; no blocked findings remain.
    Completed,
    /// Some findings were repaired; at least one blocked finding was left untouched.
    Partial,
    /// Reserved for future use; not currently produced by `execute_sync`.
    Failed,
}

/// Resolution log record appended to `.synrepo/state/repair-log.jsonl`
/// for each mutating `sync` run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolutionLogEntry {
    /// RFC 3339 UTC timestamp.
    pub synced_at: String,
    /// Git HEAD revision at the time of the sync, if available.
    pub source_revision: Option<String>,
    /// Surfaces that were in scope for this sync run.
    pub requested_scope: Vec<RepairSurface>,
    /// All findings the sync pass evaluated.
    pub findings_considered: Vec<RepairFinding>,
    /// Human-readable description of each action taken.
    pub actions_taken: Vec<String>,
    /// Final outcome of the sync run.
    pub outcome: SyncOutcome,
}

/// Structured progress event emitted while `execute_sync_locked` runs.
///
/// Serialized across the watch event channel and the watch control socket, so
/// every variant must be plain serde-compatible data. Commentary-level
/// granularity is exposed as summary counters rather than the internal
/// `CommentaryProgressEvent` to keep the wire format stable.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncProgress {
    /// A surface handler is about to run. Emitted before dispatch.
    SurfaceStarted {
        /// Surface the handler is about to repair.
        surface: RepairSurface,
        /// Action the handler will attempt.
        action: RepairAction,
    },
    /// A surface handler has returned. Emitted after dispatch.
    SurfaceFinished {
        /// Surface that just ran.
        surface: RepairSurface,
        /// How the surface bucketed in the summary.
        outcome: SurfaceOutcome,
    },
    /// Commentary refresh planning complete.
    CommentaryPlan {
        /// Entries scheduled for refresh because the source hash changed.
        refresh: usize,
        /// File-scope seeds scheduled.
        file_seeds: usize,
        /// Symbol-scope seeds scheduled.
        symbol_seed_candidates: usize,
    },
    /// Commentary refresh is making per-target progress.
    CommentaryItem {
        /// Current target index (1-based).
        current: usize,
        /// Whether the generator produced content for this target.
        generated: bool,
    },
    /// Commentary refresh completed. Counts mirror the internal summary.
    CommentarySummary {
        /// Entries refreshed successfully.
        refreshed: usize,
        /// New seeds materialized.
        seeded: usize,
        /// Targets attempted but not produced (budget or skip).
        not_generated: usize,
        /// Total targets attempted.
        attempted: usize,
        /// True if the operator requested a stop partway through.
        stopped: bool,
    },
}

/// How a surface bucketed in the resulting `SyncSummary`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceOutcome {
    /// Action succeeded; finding moved to `repaired`.
    Repaired,
    /// Action was report-only; finding moved to `report_only`.
    ReportOnly,
    /// Action was blocked; finding moved to `blocked`.
    Blocked,
    /// Finding was skipped because a surface filter excluded it.
    FilteredOut,
}
