//! View model struct definitions for the probe module.

/// Severity tag used by the dashboard to pick a color token and pane ordering.
/// `Healthy` is the baseline; `Stale` and `Blocked` escalate attention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    /// Healthy, no action required.
    Healthy,
    /// Stale or degraded, worth noting but not blocking.
    Stale,
    /// Blocked or error — operator action required.
    Blocked,
}

impl Severity {
    /// Stable lowercase tag used by `synrepo doctor` JSON output and any other
    /// serializer that needs a consistent representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Healthy => "healthy",
            Severity::Stale => "stale",
            Severity::Blocked => "blocked",
        }
    }
}

/// Flattened header view model consumed by the header widget.
#[derive(Clone, Debug)]
pub struct HeaderVm {
    /// Human-readable repo path.
    pub repo_display: String,
    /// Mode label (`auto`, `curated`, `unknown`).
    pub mode_label: String,
    /// Reconcile health summary.
    pub reconcile_label: String,
    /// Reconcile severity.
    pub reconcile_severity: Severity,
    /// Watch summary.
    pub watch_label: String,
    /// Watch severity (Healthy when running or cleanly inactive, Stale for
    /// stale artifacts, Blocked on corrupt state).
    pub watch_severity: Severity,
    /// Writer-lock summary.
    pub lock_label: String,
    /// Writer-lock severity.
    pub lock_severity: Severity,
    /// MCP readiness label ("registered", "instructions only", "absent", "n/a").
    pub mcp_label: String,
    /// MCP severity.
    pub mcp_severity: Severity,
    /// Cached auto-sync flag from the dashboard runtime. `None` for non-TUI
    /// callers (e.g. `synrepo doctor`) that have no live atomic to read.
    pub auto_sync: Option<bool>,
}

/// Flattened system-health view model.
#[derive(Clone, Debug)]
pub struct HealthVm {
    /// Rows rendered top-to-bottom.
    pub rows: Vec<HealthRow>,
}

/// One row in the system-health pane.
#[derive(Clone, Debug)]
pub struct HealthRow {
    /// Label on the left.
    pub label: String,
    /// Value on the right.
    pub value: String,
    /// Severity driving color.
    pub severity: Severity,
}

/// Trust-focused dashboard view model.
#[derive(Clone, Debug)]
pub struct TrustVm {
    /// Context-serving rows.
    pub context_rows: Vec<TrustRow>,
    /// Advisory overlay-note rows.
    pub overlay_rows: Vec<TrustRow>,
    /// Current-change impact rows.
    pub change_rows: Vec<TrustRow>,
    /// Degraded surfaces with remediation hints.
    pub degraded_rows: Vec<TrustRow>,
}

/// One row in the trust pane.
#[derive(Clone, Debug)]
pub struct TrustRow {
    /// Label on the left.
    pub label: String,
    /// Value on the right.
    pub value: String,
    /// Optional operator action or provenance hint.
    pub hint: Option<String>,
    /// Numerator for compact bar rendering.
    pub amount: Option<u64>,
    /// Denominator for compact bar rendering.
    pub total: Option<u64>,
    /// Severity driving color.
    pub severity: Severity,
}

/// Recent-activity entry reshaped for rendering.
#[derive(Clone, Debug)]
pub struct ActivityVmEntry {
    /// RFC-3339 timestamp; empty when unknown.
    pub timestamp: String,
    /// Short event kind tag.
    pub kind: String,
    /// One-line payload.
    pub payload: String,
}

/// Recent-activity view model.
#[derive(Clone, Debug, Default)]
pub struct ActivityVm {
    /// Entries newest-first.
    pub entries: Vec<ActivityVmEntry>,
}

/// One recommended next-action derived from health signals.
#[derive(Clone, Debug)]
pub struct NextAction {
    /// Short label.
    pub label: String,
    /// Severity-driven ordering hint.
    pub severity: Severity,
}
