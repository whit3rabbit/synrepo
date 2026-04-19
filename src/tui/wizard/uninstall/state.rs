//! Uninstall wizard state types.
//!
//! Input: a list of [`UninstallActionKind`] values describing everything the
//! caller detected as installed in this repo (shims, MCP entries, root
//! .gitignore lines, the `.synrepo/` directory). The wizard renders one
//! togglable row per action; the operator confirms or cancels; the wizard
//! returns an [`UninstallPlan`] listing the subset of actions that were still
//! enabled at confirm time.
//!
//! Semantics: checked = will be removed. Installed rows start checked so the
//! wizard matches the bulk-remove default. The single destructive row
//! (`DeleteSynrepoDir`) starts UNCHECKED so the operator has to opt in, the
//! same rule the repair wizard uses for `RunUpgradeApply`.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};

/// One action that may be applied by `synrepo remove`. Library-owned so the
/// wizard can be unit-tested without the bin crate, and so the bin can
/// translate its own `RemoveAction` into this and back.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UninstallActionKind {
    /// Delete an agent shim file.
    RemoveShim {
        /// Canonical tool name (e.g. "claude", "codex").
        tool: String,
        /// Absolute path to the shim file.
        path: PathBuf,
    },
    /// Remove the synrepo entry from an MCP config file, preserving unrelated
    /// entries. The file itself is never deleted.
    RemoveMcpEntry {
        /// Canonical tool name whose MCP config this entry belongs to.
        tool: String,
        /// Absolute path to the MCP config file.
        path: PathBuf,
    },
    /// Strip a single line from the root `.gitignore` (e.g. `.synrepo/`).
    RemoveGitignoreLine {
        /// The literal line to remove.
        entry: String,
    },
    /// Delete `.synrepo/` and everything inside it. Destructive; defaults off.
    DeleteSynrepoDir,
}

/// Plan produced by a completed uninstall wizard. Actions run in the order
/// they appear in the vector.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UninstallPlan {
    /// Actions the operator kept checked at confirm time.
    pub actions: Vec<UninstallActionKind>,
}

impl UninstallPlan {
    /// True when the operator unchecked every row before confirming.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// Outcome of `run_uninstall_wizard`. Shape matches the other wizards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UninstallWizardOutcome {
    /// Stdout is not a TTY; the wizard was not entered.
    NonTty,
    /// Operator cancelled before confirm. No actions to apply.
    Cancelled,
    /// Operator confirmed. Caller must execute `plan`.
    Completed {
        /// Plan to execute after the TUI alt-screen has been torn down.
        plan: UninstallPlan,
    },
}

/// Wizard screens, mirroring the repair wizard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UninstallStep {
    /// Toggle uninstall rows.
    Select,
    /// Review the plan and press Enter to apply.
    Confirm,
    /// Terminal state.
    Complete,
}

/// One togglable row in the uninstall wizard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRow {
    /// The underlying action this row represents.
    pub kind: UninstallActionKind,
    /// Operator-facing label.
    pub label: String,
    /// Current checkbox state.
    pub enabled: bool,
    /// When true, the row represents a destructive action and is rendered in
    /// the danger color. Defaults to unchecked so the operator must opt in.
    pub destructive: bool,
}

/// State machine driving the uninstall wizard.
#[derive(Clone, Debug)]
pub struct UninstallWizardState {
    /// Current screen in the Select → Confirm → Complete state machine.
    pub step: UninstallStep,
    /// Index of the currently-highlighted row on the Select screen.
    pub cursor: usize,
    /// Togglable action rows, in display order.
    pub rows: Vec<ActionRow>,
    /// Paths the remove path will NOT delete (`.mcp.json.bak` sidecars).
    /// Surfaced as guidance so the operator can see that backups are kept.
    pub preserved: Vec<PathBuf>,
    /// True when the operator quit via Esc/q/Ctrl-C. Suppresses the plan.
    pub cancelled: bool,
}

impl UninstallWizardState {
    /// Build a fresh state from the detected install surface.
    ///
    /// `installed` is the full set of actions `synrepo remove` would apply on
    /// a bulk run. Each becomes one row. All installed items start checked;
    /// `DeleteSynrepoDir` is the one exception and starts unchecked.
    pub fn new(installed: &[UninstallActionKind], preserved: &[PathBuf]) -> Self {
        let rows = installed
            .iter()
            .cloned()
            .map(|kind| {
                let destructive = matches!(kind, UninstallActionKind::DeleteSynrepoDir);
                let label = render_label(&kind);
                ActionRow {
                    kind,
                    label,
                    enabled: !destructive,
                    destructive,
                }
            })
            .collect();
        Self {
            step: UninstallStep::Select,
            cursor: 0,
            rows,
            preserved: preserved.to_vec(),
            cancelled: false,
        }
    }

    /// Handle a key event; returns `true` if the caller should redraw.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let is_quit = matches!(code, KeyCode::Esc | KeyCode::Char('q'))
            || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL));

        match self.step {
            UninstallStep::Select => {
                if is_quit {
                    self.cancelled = true;
                    self.step = UninstallStep::Complete;
                    return true;
                }
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.cursor = self.cursor.saturating_sub(1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if !self.rows.is_empty() && self.cursor + 1 < self.rows.len() {
                            self.cursor += 1;
                        }
                        true
                    }
                    KeyCode::Char(' ') => {
                        if let Some(row) = self.rows.get_mut(self.cursor) {
                            row.enabled = !row.enabled;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        self.step = UninstallStep::Confirm;
                        true
                    }
                    _ => false,
                }
            }
            UninstallStep::Confirm => match code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.step = UninstallStep::Complete;
                    true
                }
                KeyCode::Esc | KeyCode::Char('b') => {
                    self.step = UninstallStep::Select;
                    true
                }
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.cancelled = true;
                    self.step = UninstallStep::Complete;
                    true
                }
                _ => false,
            },
            UninstallStep::Complete => false,
        }
    }

    /// Produce the plan if the state machine completed without cancelling.
    pub fn finalize(&self) -> Option<UninstallPlan> {
        if self.cancelled || self.step != UninstallStep::Complete {
            return None;
        }
        let actions = self
            .rows
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.kind.clone())
            .collect();
        Some(UninstallPlan { actions })
    }
}

/// Human-readable label for a row. Destructive rows get a `!` prefix so the
/// operator sees the danger even in no-color terminals.
fn render_label(kind: &UninstallActionKind) -> String {
    match kind {
        UninstallActionKind::RemoveShim { tool, path } => {
            format!("Delete {tool} shim ({})", path.display())
        }
        UninstallActionKind::RemoveMcpEntry { tool, path } => {
            format!("Strip synrepo from {tool} MCP config ({})", path.display())
        }
        UninstallActionKind::RemoveGitignoreLine { entry } => {
            format!("Remove `{entry}` from root .gitignore")
        }
        UninstallActionKind::DeleteSynrepoDir => {
            "! Delete .synrepo/ (removes cached graph, overlay, and index data)".to_string()
        }
    }
}
