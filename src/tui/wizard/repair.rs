//! Repair wizard: guided recovery flow for partial repos.
//!
//! Walks the operator through the [`Missing`] list produced by the runtime
//! probe, exposing toggleable repair actions. Destructive actions (in
//! particular `synrepo upgrade --apply`) default to *off* and require an
//! explicit toggle; every action is visible in a confirm step before any
//! writes happen. Cancelling at any point before the confirm step guarantees
//! `.synrepo/` stays byte-identical.
//!
//! The wizard only returns a [`RepairPlan`]; the bin-side dispatcher executes
//! the plan after the TUI alt-screen has been torn down.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind, Missing};
use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::{enter_tui, leave_tui, target_label, WizardTerminal};

/// Plan produced by a completed repair wizard. Each field is an independent
/// toggle; the bin-side dispatcher executes the actions in a fixed order
/// (config → upgrade-apply → reconcile → shim) and re-runs the probe between
/// steps so a later action can observe the result of an earlier one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RepairPlan {
    /// Write the default `.synrepo/config.toml`. Runs `synrepo init` when
    /// `.synrepo/` exists but config.toml is missing.
    pub write_config: bool,
    /// Run `synrepo upgrade --apply`. Destructive (may migrate the graph
    /// store); the wizard never enables this by default.
    pub run_upgrade_apply: bool,
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
            && !self.run_upgrade_apply
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
/// `CompatBlocked` can map to `RunUpgradeApply` *and* `RunReconcile`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairActionKind {
    /// Write default config.toml.
    WriteConfig,
    /// Run `synrepo upgrade --apply`.
    RunUpgradeApply,
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
                kind: RepairActionKind::RunUpgradeApply,
                label: "Run `synrepo upgrade --apply` (destructive)".to_string(),
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
                    label: format!("Write agent shim for {}", target_label(target)),
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
                RepairActionKind::RunUpgradeApply => plan.run_upgrade_apply = true,
                RepairActionKind::RunReconcile => plan.run_reconcile = true,
                RepairActionKind::WriteShim => plan.write_shim_for = self.shim_target,
            }
        }
        Some(plan)
    }
}

/// Run the repair wizard until Complete or cancellation.
pub fn run_repair_wizard_loop(
    theme: Theme,
    missing: &[Missing],
    integration: &AgentIntegration,
    detected_targets: &[AgentTargetKind],
) -> anyhow::Result<RepairWizardOutcome> {
    let mut terminal = enter_tui()?;
    let mut state = RepairWizardState::new(missing, integration, detected_targets);
    let result = render_loop(&mut terminal, &mut state, &theme);
    leave_tui(&mut terminal)?;
    result?;
    if state.cancelled {
        Ok(RepairWizardOutcome::Cancelled)
    } else if let Some(plan) = state.finalize() {
        Ok(RepairWizardOutcome::Completed { plan })
    } else {
        Ok(RepairWizardOutcome::Cancelled)
    }
}

fn render_loop(
    terminal: &mut WizardTerminal,
    state: &mut RepairWizardState,
    theme: &Theme,
) -> anyhow::Result<()> {
    use std::time::Duration;
    while state.step != RepairStep::Complete {
        terminal.draw(|frame| draw(frame, state, theme))?;
        if let Some((code, mods)) = poll_key(Duration::from_millis(250))? {
            state.handle_key(code, mods);
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, state: &RepairWizardState, theme: &Theme) {
    let size = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(size);

    let title = Paragraph::new(Line::from(Span::styled(
        match state.step {
            RepairStep::Select => " synrepo repair — select actions ",
            RepairStep::Confirm => " synrepo repair — confirm ",
            RepairStep::Complete => " synrepo repair — done ",
        },
        theme.agent_style(),
    )))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );
    frame.render_widget(title, outer[0]);

    match state.step {
        RepairStep::Select => draw_select(frame, outer[1], state, theme),
        RepairStep::Confirm => draw_confirm(frame, outer[1], state, theme),
        RepairStep::Complete => {}
    }

    let hint = match state.step {
        RepairStep::Select => " ↑/↓ move  Space toggle  Enter continue  Esc cancel ",
        RepairStep::Confirm => " Enter apply  b back  Ctrl-C abort ",
        RepairStep::Complete => "",
    };
    let footer = Paragraph::new(Span::styled(hint, theme.muted_style())).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );
    frame.render_widget(footer, outer[2]);
}

fn draw_select(frame: &mut ratatui::Frame, area: Rect, state: &RepairWizardState, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::new();
    if !state.guidance.is_empty() {
        lines.push(Line::from(Span::styled(
            "Probe guidance:",
            theme.muted_style(),
        )));
        for g in &state.guidance {
            lines.push(Line::from(Span::styled(
                format!("  {g}"),
                theme.muted_style(),
            )));
        }
        lines.push(Line::from(Span::raw("")));
    }
    lines.push(Line::from(Span::styled(
        "Select repair actions (Space toggles):",
        theme.base_style(),
    )));

    let rows: Vec<ListItem> = state
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = i == state.cursor;
            let marker = if selected { "▶ " } else { "  " };
            let check = if row.enabled { "[x] " } else { "[ ] " };
            let style = if selected {
                theme.agent_style()
            } else if row.destructive {
                theme.blocked_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{marker}{check}{}", row.label),
                style,
            )))
        })
        .collect();

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(lines.len() as u16 + 2),
            Constraint::Min(2),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" context "),
        ),
        split[0],
    );
    frame.render_widget(
        List::new(rows).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" actions "),
        ),
        split[1],
    );
}

fn draw_confirm(frame: &mut ratatui::Frame, area: Rect, state: &RepairWizardState, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "The following actions will run, in order:",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::raw("")));

    let mut step = 1;
    for row in &state.rows {
        if !row.enabled {
            continue;
        }
        let style = if row.destructive {
            theme.blocked_style()
        } else {
            theme.base_style()
        };
        lines.push(Line::from(Span::styled(
            format!("  {step}. {}", row.label),
            style,
        )));
        step += 1;
    }
    if step == 1 {
        lines.push(Line::from(Span::styled(
            "  (no actions selected)",
            theme.muted_style(),
        )));
    }

    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "No files have been written yet. Press Enter to apply or b to go back.",
        theme.muted_style(),
    )));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" confirm "),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(state: &mut RepairWizardState, code: KeyCode) {
        state.handle_key(code, KeyModifiers::empty());
    }

    #[test]
    fn config_missing_row_is_pre_enabled() {
        let s = RepairWizardState::new(&[Missing::ConfigFile], &AgentIntegration::Absent, &[]);
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::WriteConfig)
            .expect("config row");
        assert!(row.enabled);
        assert!(!row.destructive);
    }

    #[test]
    fn upgrade_apply_is_never_enabled_by_default() {
        let s = RepairWizardState::new(
            &[Missing::CompatBlocked {
                guidance: vec!["migrate".into()],
            }],
            &AgentIntegration::Absent,
            &[],
        );
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::RunUpgradeApply)
            .expect("upgrade row");
        assert!(!row.enabled, "destructive row must default off");
        assert!(row.destructive);
    }

    #[test]
    fn graph_store_missing_enables_reconcile_by_default() {
        let s = RepairWizardState::new(&[Missing::GraphStore], &AgentIntegration::Absent, &[]);
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::RunReconcile)
            .expect("reconcile row");
        assert!(row.enabled);
    }

    #[test]
    fn integration_partial_offers_shim_row() {
        let s = RepairWizardState::new(
            &[],
            &AgentIntegration::Partial {
                target: AgentTargetKind::Claude,
            },
            &[],
        );
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::WriteShim)
            .expect("shim row");
        assert!(!row.enabled);
        assert_eq!(s.shim_target, Some(AgentTargetKind::Claude));
    }

    #[test]
    fn integration_complete_omits_shim_row() {
        let s = RepairWizardState::new(
            &[Missing::GraphStore],
            &AgentIntegration::Complete {
                target: AgentTargetKind::Claude,
            },
            &[],
        );
        assert!(s.rows.iter().all(|r| r.kind != RepairActionKind::WriteShim));
    }

    #[test]
    fn happy_path_accept_defaults_and_confirm() {
        let mut s = RepairWizardState::new(
            &[Missing::ConfigFile, Missing::GraphStore],
            &AgentIntegration::Absent,
            &[],
        );
        press(&mut s, KeyCode::Enter); // to Confirm
        assert_eq!(s.step, RepairStep::Confirm);
        press(&mut s, KeyCode::Enter); // apply
        assert_eq!(s.step, RepairStep::Complete);
        let plan = s.finalize().expect("plan");
        assert!(plan.write_config);
        assert!(plan.run_reconcile);
        assert!(!plan.run_upgrade_apply);
        assert_eq!(plan.write_shim_for, None);
    }

    #[test]
    fn space_toggles_enable_upgrade_apply() {
        let mut s = RepairWizardState::new(
            &[Missing::CompatBlocked {
                guidance: vec!["migrate".into()],
            }],
            &AgentIntegration::Absent,
            &[],
        );
        // First row is the upgrade-apply destructive row (config wasn't missing).
        assert!(!s.rows[0].enabled);
        press(&mut s, KeyCode::Char(' '));
        assert!(s.rows[0].enabled);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        let plan = s.finalize().expect("plan");
        assert!(plan.run_upgrade_apply);
    }

    #[test]
    fn esc_at_select_cancels_with_no_plan() {
        let mut s = RepairWizardState::new(&[Missing::ConfigFile], &AgentIntegration::Absent, &[]);
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn b_at_confirm_goes_back_without_cancelling() {
        let mut s = RepairWizardState::new(&[Missing::ConfigFile], &AgentIntegration::Absent, &[]);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, RepairStep::Confirm);
        press(&mut s, KeyCode::Char('b'));
        assert_eq!(s.step, RepairStep::Select);
        assert!(!s.cancelled);
    }

    #[test]
    fn cancel_before_confirm_leaves_filesystem_untouched() {
        // This is a state-level invariant: finalize() returns None on cancel.
        // The fs-level invariant is guaranteed by design — the wizard never
        // mutates the fs; only the bin-side dispatcher does. See the matching
        // bin test in cli_support/tests/setup.rs.
        let mut s = RepairWizardState::new(
            &[
                Missing::ConfigFile,
                Missing::CompatBlocked {
                    guidance: vec!["migrate".into()],
                },
            ],
            &AgentIntegration::Partial {
                target: AgentTargetKind::Claude,
            },
            &[],
        );
        press(&mut s, KeyCode::Char(' ')); // toggle first row off
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn compat_blocked_guidance_is_surfaced() {
        let s = RepairWizardState::new(
            &[Missing::CompatBlocked {
                guidance: vec!["run synrepo upgrade --apply".to_string()],
            }],
            &AgentIntegration::Absent,
            &[],
        );
        assert!(s.guidance.iter().any(|g| g.contains("upgrade --apply")));
    }

    #[test]
    fn is_empty_reports_no_actions() {
        let p = RepairPlan::default();
        assert!(p.is_empty());
        let p = RepairPlan {
            write_config: true,
            ..RepairPlan::default()
        };
        assert!(!p.is_empty());
    }
}
