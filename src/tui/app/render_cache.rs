//! Cached render inputs that should not be rebuilt every frame.

use std::path::Path;
use std::time::Instant;

use super::{quick_actions_for, ActiveTab, AppState};
use crate::surface::refactor_suggestions::{
    collect_refactor_suggestions_for_repo, RefactorSuggestionOptions,
};
use crate::surface::status_snapshot::{build_status_snapshot, StatusOptions};
use crate::tui::agent_integrations::{
    build_agent_install_display_rows, build_agent_install_statuses,
    summarize_agent_install_statuses, AgentInstallDisplayRow, AgentInstallStatus,
};
use crate::tui::probe::{build_header_vm, display_repo_path, HeaderVm};
use crate::tui::widgets::LogEntry;

pub(super) fn build_initial_header_vm(
    repo_root: &Path,
    project_name: Option<&str>,
    snapshot: &crate::surface::status_snapshot::StatusSnapshot,
    integration: &crate::bootstrap::runtime_probe::AgentIntegration,
    auto_sync_enabled: bool,
    integration_status_rows: &[AgentInstallStatus],
) -> HeaderVm {
    let mut header = build_header_vm(
        repo_display(repo_root, project_name),
        snapshot,
        integration,
        Some(auto_sync_enabled),
    );
    let summary = summarize_agent_install_statuses(integration_status_rows);
    header.mcp_label = summary.label;
    header.mcp_severity = summary.severity;
    header
}

pub(super) fn build_initial_integration_display_rows(
    rows: &[AgentInstallStatus],
) -> Vec<AgentInstallDisplayRow> {
    build_agent_install_display_rows(rows)
}

/// Compose the header repo label as `"<project>  <path>"` when a project name
/// is set, otherwise just the path. Crate-public so the global Repos chrome
/// renders the same label as a single-project dashboard.
pub(crate) fn repo_display(repo_root: &Path, project_name: Option<&str>) -> String {
    let repo_path = display_repo_path(repo_root);
    project_name
        .map(|name| format!("{name}  {repo_path}"))
        .unwrap_or(repo_path)
}

impl AppState {
    /// Rebuild header labels after snapshot, project identity, or auto-sync
    /// state changes. The draw loop only reads this cached view model.
    pub(crate) fn rebuild_header_vm(&mut self) {
        let selected_tool = self
            .integration_display_rows
            .get(self.integration_selected_index())
            .map(|row| row.tool.clone());
        let integration_status_rows = build_agent_install_statuses(&self.repo_root);
        self.header_vm = build_initial_header_vm(
            &self.repo_root,
            self.project_name.as_deref(),
            &self.snapshot,
            &self.integration,
            self.auto_sync_enabled,
            &integration_status_rows,
        );
        self.integration_display_rows = build_agent_install_display_rows(&integration_status_rows);
        self.preserve_integration_selection(selected_tool.as_deref());
    }

    /// Load suggestion rows only when the tab needs them.
    pub(crate) fn ensure_suggestions_loaded(&mut self) {
        if self.suggestion_report.is_none() {
            self.load_suggestions(false);
        }
    }

    /// Refresh large-file suggestions and show an operator-visible toast.
    pub(crate) fn refresh_suggestions(&mut self) {
        self.load_suggestions(true);
    }

    fn load_suggestions(&mut self, toast: bool) {
        match collect_refactor_suggestions_for_repo(
            &self.repo_root,
            RefactorSuggestionOptions::default(),
        ) {
            Ok(report) => {
                let count = report.candidate_count;
                self.suggestion_report = Some(report);
                if toast {
                    self.set_toast(format!("suggestions refreshed: {count} candidates"));
                }
            }
            Err(error) => {
                self.suggestion_report = None;
                self.set_toast(format!("suggestions unavailable: {error}"));
            }
        }
    }

    /// Force a snapshot refresh right now.
    pub fn refresh_now(&mut self) {
        self.snapshot = build_status_snapshot(
            &self.repo_root,
            StatusOptions {
                recent: true,
                full: false,
            },
        );
        self.quick_actions = quick_actions_for(&self.mode, &self.snapshot);
        self.rebuild_header_vm();
        if matches!(self.active_tab, ActiveTab::Explain) {
            self.refresh_explain_preview(false);
        }
        self.last_refresh = Instant::now();
    }

    /// Push the one-shot welcome banner entry that appears on first transition
    /// from the setup wizard.
    pub fn push_welcome_banner(&mut self) {
        self.log.push(LogEntry {
            timestamp: crate::tui::actions::now_rfc3339(),
            tag: "synrepo".to_string(),
            message:
                "Welcome to synrepo. Press q to quit, r to refresh. Run `synrepo --help` for more."
                    .to_string(),
            severity: crate::tui::probe::Severity::Healthy,
        });
    }
}
