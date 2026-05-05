//! Repo-local MCP install picker state.

use std::path::{Path, PathBuf};

use agent_config::{InstallStatus, Scope, ScopeKind};
use crossterm::event::{KeyCode, KeyModifiers};

use crate::agent_install::{SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::bootstrap::runtime_probe::{all_agent_targets, AgentTargetKind};
use crate::tui::mcp_status::{McpScope, McpStatus, McpStatusRow};

/// Plan produced by a completed MCP install picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpInstallPlan {
    /// Target selected by the operator.
    pub target: AgentTargetKind,
}

/// Outcome of the MCP install picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum McpInstallWizardOutcome {
    /// Stdout is not a TTY; picker was not entered.
    NonTty,
    /// Operator cancelled before confirm; no writes performed.
    Cancelled,
    /// Operator confirmed; caller must execute `plan`.
    Completed {
        /// Plan to execute.
        plan: McpInstallPlan,
    },
}

/// Steps the picker walks through.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpInstallStep {
    /// Pick an MCP-capable local target.
    SelectTarget,
    /// Review and apply.
    Confirm,
    /// Terminal state; render loop exits.
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct McpInstallTargetRow {
    pub(crate) target: AgentTargetKind,
    pub(crate) agent: String,
    pub(crate) status: McpStatus,
    pub(crate) scope: McpScope,
    pub(crate) source: String,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) detected: bool,
}

/// State machine driving the picker.
#[derive(Clone, Debug)]
pub struct McpInstallWizardState {
    /// Current step.
    pub step: McpInstallStep,
    rows: Vec<McpInstallTargetRow>,
    /// Cursor in `rows`.
    pub target_cursor: usize,
    /// Committed target.
    pub target: AgentTargetKind,
    /// True when the operator pressed Esc / q / Ctrl-C before Confirm.
    pub cancelled: bool,
}

impl McpInstallWizardState {
    /// Build a fresh state seeded from active-project MCP rows.
    pub fn new(
        repo_root: &Path,
        status_rows: Vec<McpStatusRow>,
        detected_targets: Vec<AgentTargetKind>,
    ) -> Self {
        let rows = local_target_rows(repo_root, status_rows, &detected_targets);
        let target_cursor = default_cursor(&rows);
        let target = rows
            .get(target_cursor)
            .map(|row| row.target)
            .unwrap_or(AgentTargetKind::Codex);
        Self {
            step: McpInstallStep::SelectTarget,
            rows,
            target_cursor,
            target,
            cancelled: false,
        }
    }

    /// Rows displayed by the picker.
    pub(crate) fn rows(&self) -> &[McpInstallTargetRow] {
        &self.rows
    }

    /// Current row, if any local MCP-capable target exists.
    pub(crate) fn selected_row(&self) -> Option<&McpInstallTargetRow> {
        self.rows.get(self.target_cursor)
    }

    /// Handle a key event. Returns `true` when the event was consumed and the
    /// caller should redraw.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match self.step {
            McpInstallStep::SelectTarget => self.handle_select_target(code, modifiers),
            McpInstallStep::Confirm => self.handle_confirm(code, modifiers),
            McpInstallStep::Complete => false,
        }
    }

    /// Return the completed plan when the operator confirmed.
    pub fn finalize(&self) -> Option<McpInstallPlan> {
        if self.cancelled || self.step != McpInstallStep::Complete {
            return None;
        }
        Some(McpInstallPlan {
            target: self.target,
        })
    }

    fn is_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
        code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
    }

    fn handle_select_target(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if matches!(code, KeyCode::Esc | KeyCode::Char('q')) || Self::is_ctrl_c(code, modifiers) {
            self.cancelled = true;
            self.step = McpInstallStep::Complete;
            return true;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.target_cursor = self.target_cursor.saturating_sub(1);
                self.reseed_target();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.target_cursor + 1 < self.rows.len() {
                    self.target_cursor += 1;
                    self.reseed_target();
                }
                true
            }
            KeyCode::Enter if !self.rows.is_empty() => {
                self.reseed_target();
                self.step = McpInstallStep::Confirm;
                true
            }
            _ => false,
        }
    }

    fn handle_confirm(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if code == KeyCode::Char('q') || Self::is_ctrl_c(code, modifiers) {
            self.cancelled = true;
            self.step = McpInstallStep::Complete;
            return true;
        }
        match code {
            KeyCode::Esc | KeyCode::Char('b') => {
                self.step = McpInstallStep::SelectTarget;
                true
            }
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.step = McpInstallStep::Complete;
                true
            }
            _ => false,
        }
    }

    fn reseed_target(&mut self) {
        if let Some(row) = self.selected_row() {
            self.target = row.target;
        }
    }
}

fn local_target_rows(
    repo_root: &Path,
    status_rows: Vec<McpStatusRow>,
    detected_targets: &[AgentTargetKind],
) -> Vec<McpInstallTargetRow> {
    status_rows
        .into_iter()
        .filter_map(|row| {
            let target = target_for_id(&row.tool)?;
            let installer = agent_config::mcp_by_id(&row.tool)?;
            if !installer.supported_mcp_scopes().contains(&ScopeKind::Local) {
                return None;
            }
            let local_scope = Scope::Local(repo_root.to_path_buf());
            let local_report = installer
                .mcp_status(&local_scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
                .ok();
            let (status, scope, source) = match local_report.as_ref().map(|report| &report.status) {
                Some(InstallStatus::InstalledOwned { .. }) => (
                    McpStatus::Registered,
                    McpScope::Project,
                    "agent-config owned",
                ),
                Some(InstallStatus::PresentUnowned) => {
                    (McpStatus::Registered, McpScope::Project, "legacy config")
                }
                _ => (
                    McpStatus::Missing,
                    McpScope::Missing,
                    detected_source(detected_targets.contains(&target)),
                ),
            };
            let config_path = local_report
                .and_then(|report| report.config_path)
                .or_else(|| {
                    (row.scope == McpScope::Project)
                        .then_some(row.config_path)
                        .flatten()
                });
            Some(McpInstallTargetRow {
                target,
                agent: row.agent,
                status,
                scope,
                source: source.to_string(),
                config_path,
                detected: detected_targets.contains(&target),
            })
        })
        .collect()
}

fn default_cursor(rows: &[McpInstallTargetRow]) -> usize {
    rows.iter()
        .position(|row| row.status == McpStatus::Missing && row.detected)
        .or_else(|| rows.iter().position(|row| row.status == McpStatus::Missing))
        .or_else(|| rows.iter().position(|row| row.detected))
        .unwrap_or(0)
}

fn target_for_id(id: &str) -> Option<AgentTargetKind> {
    all_agent_targets()
        .iter()
        .copied()
        .find(|target| target.as_str() == id)
}

fn detected_source(detected: bool) -> &'static str {
    if detected {
        "target hint"
    } else {
        "not detected"
    }
}
