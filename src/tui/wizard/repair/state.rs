//! Repair wizard state types.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind, Missing};

/// Plan produced by a completed repair wizard. Each field is an independent
/// toggle; the bin-side dispatcher executes the actions in a fixed order
/// (config → recreate-runtime → reconcile → shim) and re-runs the probe
/// between steps so a later action can observe the result of an earlier one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RepairPlan {
    /// Write the default `.synrepo/config.toml`. Runs `synrepo init` when
    /// `.synrepo/` exists but config.toml is missing.
    pub write_config: bool,
    /// Run `synrepo init --force` to recreate `.synrepo/` in place. Used when
    /// the canonical graph store is incompatible with this binary and cannot
    /// be migrated. Destructive; the wizard never enables this by default.
    pub recreate_runtime: bool,
    /// Run a reconcile pass to populate the graph store.
    pub run_reconcile: bool,
    /// Write the agent shim for the given target. `None` means no shim write.
    pub write_shim_for: Option<AgentTargetKind>,
}

impl RepairPlan {
    /// True when every toggle is off — useful to the bin-side dispatcher so it
    /// can print "nothing to do" instead of silently doing nothing.
    pub fn is_empty(&self) -> bool {
        !self.write_config
            && !self.recreate_runtime
            && !self.run_reconcile
            && self.write_shim_for.is_none()
    }
}

/// Outcome returned by the repair wizard. Matches the shape of
/// [`crate::tui::wizard::SetupWizardOutcome`] so the bin-side dispatcher has a
/// uniform match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairWizardOutcome {
    /// Stdout is not a TTY; the wizard was not entered.
    NonTty,
    /// Operator cancelled before confirm. No writes performed.
    Cancelled,
    /// Operator confirmed; caller must execute `plan`.
    Completed {
        /// Plan to execute.
        plan: RepairPlan,
    },
}

/// Repair wizard steps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairStep {
    /// Toggle repair actions.
    Select,
    /// Review the plan and press Enter to apply.
    Confirm,
    /// Terminal state.
    Complete,
}

/// One toggleable row in the repair wizard. Kept small so the state struct
/// can own its own `Vec<ActionRow>` rather than re-deriving rows per key
/// event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRow {
    /// Short machine-readable identifier (tests assert on this).
    pub kind: RepairActionKind,
    /// Operator-facing label.
    pub label: String,
    /// Current checkbox state.
    pub enabled: bool,
    /// True when toggling this row requires extra confirmation (Space twice).
    /// The wizard does not auto-enable destructive rows.
    pub destructive: bool,
}

/// Kinds of repair actions the wizard can propose. Distinct from `Missing`
/// because one missing component can map to multiple actions (e.g.
/// `CompatBlocked` can map to `RecreateRuntime` *and* `RunReconcile`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairActionKind {
    /// Write default config.toml.
    WriteConfig,
    /// Recreate `.synrepo/` in place via `synrepo init --force`. Replaces the
    /// previous `RunUpgradeApply` action: `synrepo upgrade` cannot migrate a
    /// canonical-store `Block`, so the wizard now offers the destructive
    /// recreate path instead.
    RecreateRuntime,
    /// Run reconcile pass.
    RunReconcile,
    /// Write agent integration shim for the selected target.
    WriteShim,
}

/// State machine driving the repair wizard.
#[derive(Clone, Debug)]
pub struct RepairWizardState {
    /// Current step.
    pub step: RepairStep,
    /// Cursor index into `rows`.
    pub cursor: usize,
    /// Current row set derived from the probe's `Missing` list + integration.
    pub rows: Vec<ActionRow>,
    /// Target to associate with `WriteShim` when enabled. Seeded from the
    /// first detected target in the probe report; wizard never reads it
    /// unless `WriteShim` is enabled.
    pub shim_target: Option<AgentTargetKind>,
    /// Guidance lines lifted from the probe report; rendered above the rows.
    pub guidance: Vec<String>,
    /// True when the operator pressed Esc / Ctrl-C / q before Confirm.
    pub cancelled: bool,
}

impl RepairWizardState {
    /// Build a fresh state from the probe's classification payload. The caller
    /// supplies `missing` (from `RuntimeClassification::Partial`) and
    /// `integration` (from the same probe report) so the wizard can propose
    /// both required-runtime and optional agent-integration rows.
    pub fn new(
        missing: &[Missing],
        integration: &AgentIntegration,
        detected_targets: &[AgentTargetKind],
    ) -> Self {
        let mut rows: Vec<ActionRow> = Vec::new();
        let mut guidance: Vec<String> = Vec::new();

        let mut has_config_file = false;
        let mut has_graph_store = false;
        let mut has_compat_blocked = false;

        for m in missing {
            match m {
                Missing::ConfigFile => has_config_file = true,
                Missing::ConfigUnreadable { detail } => {
                    guidance.push(format!("config.toml unreadable: {detail}"));
                }
                Missing::GraphStore => has_graph_store = true,
                Missing::CompatBlocked { guidance: lines } => {
                    has_compat_blocked = true;
                    for line in lines {
                        guidance.push(format!("compat: {line}"));
                    }
                }
                Missing::CompatEvaluationFailed { detail } => {
                    guidance.push(format!("compat evaluation failed: {detail}"));
                }
            }
        }

        if has_config_file {
            rows.push(ActionRow {
                kind: RepairActionKind::WriteConfig,
                label: "Write default .synrepo/config.toml".to_string(),
                enabled: true,
                destructive: false,
            });
        }
        if has_compat_blocked {
            rows.push(ActionRow {
                kind: RepairActionKind::RecreateRuntime,
                label: "Recreate .synrepo/ via `init --force` (destructive)".to_string(),
                enabled: false,
                destructive: true,
            });
        }
        if has_graph_store || has_compat_blocked {
            rows.push(ActionRow {
                kind: RepairActionKind::RunReconcile,
                label: "Run reconcile pass".to_string(),
                enabled: has_graph_store,
                destructive: false,
            });
        }

        let shim_target = integration
            .target()
            .or_else(|| detected_targets.first().copied());

        if matches!(
            integration,
            AgentIntegration::Absent | AgentIntegration::Partial { .. }
        ) {
            if let Some(target) = shim_target {
                rows.push(ActionRow {
                    kind: RepairActionKind::WriteShim,
                    label: format!(
                        "Write {} {}",
                        crate::tui::wizard::target_label(target),
                        crate::tui::wizard::target_artifact_label(target)
                    ),
                    enabled: false,
                    destructive: false,
                });
            }
        }

        Self {
            step: RepairStep::Select,
            cursor: 0,
            rows,
            shim_target,
            guidance,
            cancelled: false,
        }
    }

    /// Handle a key event; returns `true` if the caller should redraw.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let is_quit = matches!(code, KeyCode::Esc | KeyCode::Char('q'))
            || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL));

        match self.step {
            RepairStep::Select => {
                if is_quit {
                    self.cancelled = true;
                    self.step = RepairStep::Complete;
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
                        self.step = RepairStep::Confirm;
                        true
                    }
                    _ => false,
                }
            }
            RepairStep::Confirm => match code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.step = RepairStep::Complete;
                    true
                }
                KeyCode::Esc | KeyCode::Char('b') => {
                    self.step = RepairStep::Select;
                    true
                }
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.cancelled = true;
                    self.step = RepairStep::Complete;
                    true
                }
                _ => false,
            },
            RepairStep::Complete => false,
        }
    }

    /// Produce the plan if the state machine completed without cancelling.
    pub fn finalize(&self) -> Option<RepairPlan> {
        if self.cancelled || self.step != RepairStep::Complete {
            return None;
        }
        let mut plan = RepairPlan::default();
        for row in &self.rows {
            if !row.enabled {
                continue;
            }
            match row.kind {
                RepairActionKind::WriteConfig => plan.write_config = true,
                RepairActionKind::RecreateRuntime => plan.recreate_runtime = true,
                RepairActionKind::RunReconcile => plan.run_reconcile = true,
                RepairActionKind::WriteShim => plan.write_shim_for = self.shim_target,
            }
        }
        Some(plan)
    }
}
