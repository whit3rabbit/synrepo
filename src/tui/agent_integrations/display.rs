use crate::tui::probe::Severity;

use super::{
    AgentInstallStatus, AgentOverallStatus, ComponentStatus, HookInstallStatus, HookStatus,
    IntegrationComponent,
};

/// Preformatted row cells used by the dashboard render path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentInstallDisplayRow {
    /// Stable tool id such as `claude` or `codex`.
    pub tool: String,
    /// Agent cell.
    pub agent: String,
    /// Overall status label.
    pub overall_label: &'static str,
    /// Overall severity.
    pub overall_severity: Severity,
    /// Context column.
    pub context: String,
    /// Context severity.
    pub context_severity: Severity,
    /// MCP column.
    pub mcp: String,
    /// MCP severity.
    pub mcp_severity: Severity,
    /// Hooks column.
    pub hooks: String,
    /// Hooks severity. Missing hooks stay healthy because they are optional.
    pub hooks_severity: Severity,
    /// Next-action column.
    pub next_action: String,
}

/// Compact integration status used by the dashboard header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentInstallSummary {
    /// Header label, excluding the `integrations:` prefix.
    pub label: String,
    /// Header severity.
    pub severity: Severity,
}

impl AgentInstallDisplayRow {
    fn from_status(row: &AgentInstallStatus) -> Self {
        Self {
            tool: row.tool.clone(),
            agent: format!("{}{}", row.display_name, detected_suffix(row.detected)),
            overall_label: row.overall.as_str(),
            overall_severity: overall_severity(row.overall),
            context: component_label(&row.context),
            context_severity: component_severity(&row.context),
            mcp: component_label(&row.mcp),
            mcp_severity: component_severity(&row.mcp),
            hooks: hook_label(&row.hooks),
            hooks_severity: hook_severity(row.hooks.status),
            next_action: row.next_action.clone(),
        }
    }
}

/// Preformat integration rows once per status refresh instead of every frame.
pub fn build_agent_install_display_rows(
    rows: &[AgentInstallStatus],
) -> Vec<AgentInstallDisplayRow> {
    rows.iter()
        .map(AgentInstallDisplayRow::from_status)
        .collect()
}

/// Summarize detailed integration rows for the compact dashboard header.
pub fn summarize_agent_install_statuses(rows: &[AgentInstallStatus]) -> AgentInstallSummary {
    let complete: Vec<&AgentInstallStatus> = rows
        .iter()
        .filter(|row| row.overall == AgentOverallStatus::Complete)
        .collect();
    if complete.len() == 1 {
        return AgentInstallSummary {
            label: format!("complete ({})", complete[0].tool),
            severity: Severity::Healthy,
        };
    }
    if complete.len() > 1 {
        return AgentInstallSummary {
            label: format!("complete ({})", complete.len()),
            severity: Severity::Healthy,
        };
    }

    if let Some(row) = rows
        .iter()
        .find(|row| row.overall == AgentOverallStatus::Partial)
    {
        return AgentInstallSummary {
            label: format!("partial ({})", row.tool),
            severity: Severity::Stale,
        };
    }

    AgentInstallSummary {
        label: "absent".to_string(),
        severity: Severity::Stale,
    }
}

fn detected_suffix(detected: bool) -> &'static str {
    if detected {
        " (detected)"
    } else {
        ""
    }
}

fn component_label(component: &IntegrationComponent) -> String {
    let mut out = format!(
        "{} {} {}",
        component.kind.as_str(),
        component.status.as_str(),
        component.scope.as_str()
    );
    out.push_str(&format!(" {}", component.source));
    if let Some(path) = &component.path {
        out.push_str(&format!(" {}", path.display()));
    }
    out
}

fn hook_label(hooks: &HookInstallStatus) -> String {
    let mut out = hooks.status.as_str().to_string();
    out.push_str(&format!(" {}", hooks.source));
    if let Some(path) = &hooks.path {
        out.push_str(&format!(" {}", path.display()));
    }
    out
}

fn component_severity(component: &IntegrationComponent) -> Severity {
    match component.status {
        ComponentStatus::Installed => Severity::Healthy,
        ComponentStatus::Missing | ComponentStatus::Unsupported => Severity::Stale,
    }
}

fn hook_severity(status: HookStatus) -> Severity {
    match status {
        HookStatus::Installed | HookStatus::Missing | HookStatus::Unsupported => Severity::Healthy,
    }
}

fn overall_severity(status: AgentOverallStatus) -> Severity {
    match status {
        AgentOverallStatus::Complete => Severity::Healthy,
        AgentOverallStatus::Partial
        | AgentOverallStatus::Missing
        | AgentOverallStatus::Unsupported => Severity::Stale,
    }
}
