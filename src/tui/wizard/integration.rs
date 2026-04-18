//! Agent-integration sub-wizard. Launched from the dashboard quick action so
//! operators can write a shim, register the MCP server, or both — without
//! leaving the TUI. Destructive actions (overwriting an existing shim whose
//! content differs from canonical) are gated behind an explicit toggle.
//!
//! Like the other wizards in this module, this one produces an
//! [`IntegrationPlan`] that the bin-side dispatcher executes after the TUI
//! alt-screen tears down. The library never calls the bin-side `step_*`
//! helpers directly.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::setup::WIZARD_TARGETS;
use crate::tui::wizard::{enter_tui, leave_tui, target_label, WizardTerminal};

/// Plan produced by a completed integration sub-wizard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationPlan {
    /// Target selected by the operator.
    pub target: AgentTargetKind,
    /// Write (or update) the agent shim on disk.
    pub write_shim: bool,
    /// Register the synrepo MCP server in the target agent's config.
    pub register_mcp: bool,
    /// If true, overwrite an existing shim whose content differs from the
    /// canonical template. Mirrors `synrepo agent-setup --regen`. Never true
    /// by default; the operator must explicitly opt in.
    pub overwrite_shim: bool,
}

/// Outcome of the integration sub-wizard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrationWizardOutcome {
    /// Stdout is not a TTY; wizard was not entered.
    NonTty,
    /// Operator cancelled before confirm; no writes performed.
    Cancelled,
    /// Operator confirmed; caller must execute `plan`.
    Completed {
        /// Plan to execute.
        plan: IntegrationPlan,
    },
}

/// Steps the sub-wizard walks through.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationStep {
    /// Pick agent-integration target.
    SelectTarget,
    /// Toggle which actions to run (write shim / register MCP / overwrite).
    SelectActions,
    /// Review and apply.
    Confirm,
    /// Terminal state; render loop exits.
    Complete,
}

/// Action rows shown on the SelectActions step. Stored as a small array so the
/// cursor math is obvious and testable. The "overwrite" row is always present
/// but only meaningful when the selected target already has a shim on disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActionRow {
    WriteShim,
    RegisterMcp,
    OverwriteShim,
}

const ACTION_ROWS: &[ActionRow] = &[
    ActionRow::WriteShim,
    ActionRow::RegisterMcp,
    ActionRow::OverwriteShim,
];

/// State machine driving the sub-wizard. Tests drive this struct via
/// [`IntegrationWizardState::handle_key`] and assert on `finalize()` /
/// `cancelled` / `step`.
#[derive(Clone, Debug)]
pub struct IntegrationWizardState {
    /// Current step.
    pub step: IntegrationStep,
    /// Probe-derived current integration state, used to seed defaults.
    pub current: AgentIntegration,
    /// Ordered list of observationally-detected targets.
    pub detected_targets: Vec<AgentTargetKind>,
    /// Cursor in the target list (0..WIZARD_TARGETS.len()).
    pub target_cursor: usize,
    /// Committed target (set on Enter at SelectTarget).
    pub target: AgentTargetKind,
    /// Cursor in the action list (0..ACTION_ROWS.len()).
    pub action_cursor: usize,
    /// Committed action flag: write (or rewrite) the shim file for `target`.
    pub write_shim: bool,
    /// Committed action flag: register `synrepo` as an MCP server for `target`.
    pub register_mcp: bool,
    /// Committed action flag: overwrite an existing shim rather than skipping.
    pub overwrite_shim: bool,
    /// True when the operator pressed Esc / q / Ctrl-C before Confirm.
    pub cancelled: bool,
}

impl IntegrationWizardState {
    /// Build a fresh state seeded from `current` and `detected_targets`.
    pub fn new(current: AgentIntegration, detected_targets: Vec<AgentTargetKind>) -> Self {
        // Pre-highlight the configured target when one exists, otherwise the
        // first detected target, otherwise position 0.
        let configured = current.target();
        let seed = configured.or_else(|| detected_targets.first().copied());
        let target_cursor = seed
            .and_then(|t| WIZARD_TARGETS.iter().position(|wt| wt == &t))
            .unwrap_or(0);
        let target = WIZARD_TARGETS[target_cursor];

        let (write_shim, register_mcp) = default_actions_for(&current, target);
        Self {
            step: IntegrationStep::SelectTarget,
            current,
            detected_targets,
            target_cursor,
            target,
            action_cursor: 0,
            write_shim,
            register_mcp,
            overwrite_shim: false,
            cancelled: false,
        }
    }

    /// Handle a key event. Returns `true` when the event was consumed and the
    /// caller should redraw.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match self.step {
            IntegrationStep::SelectTarget => self.handle_select_target(code, modifiers),
            IntegrationStep::SelectActions => self.handle_select_actions(code, modifiers),
            IntegrationStep::Confirm => self.handle_confirm(code, modifiers),
            IntegrationStep::Complete => false,
        }
    }

    /// Shared Ctrl-C detector for per-step handlers.
    fn is_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
        code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
    }

    fn handle_select_target(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // First step: Esc / q / Ctrl-C all cancel (no prior step to go back to).
        if matches!(code, KeyCode::Esc | KeyCode::Char('q')) || Self::is_ctrl_c(code, modifiers) {
            self.cancelled = true;
            self.step = IntegrationStep::Complete;
            return true;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.target_cursor = self.target_cursor.saturating_sub(1);
                self.reseed_target();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.target_cursor + 1 < WIZARD_TARGETS.len() {
                    self.target_cursor += 1;
                    self.reseed_target();
                }
                true
            }
            KeyCode::Enter => {
                self.target = WIZARD_TARGETS[self.target_cursor];
                self.step = IntegrationStep::SelectActions;
                true
            }
            _ => false,
        }
    }

    fn reseed_target(&mut self) {
        self.target = WIZARD_TARGETS[self.target_cursor];
        let (w, m) = default_actions_for(&self.current, self.target);
        self.write_shim = w;
        self.register_mcp = m;
        self.overwrite_shim = false;
    }

    fn handle_select_actions(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // q and Ctrl-C cancel; Esc steps back to target selection.
        if code == KeyCode::Char('q') || Self::is_ctrl_c(code, modifiers) {
            self.cancelled = true;
            self.step = IntegrationStep::Complete;
            return true;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.action_cursor = self.action_cursor.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.action_cursor + 1 < ACTION_ROWS.len() {
                    self.action_cursor += 1;
                }
                true
            }
            KeyCode::Char(' ') => {
                self.toggle_action_at_cursor();
                true
            }
            KeyCode::Enter => {
                if self.any_action_selected() {
                    self.step = IntegrationStep::Confirm;
                    true
                } else {
                    // Nothing selected; refuse to advance so the confirm page
                    // never shows an empty plan. Keeps the UX honest.
                    false
                }
            }
            KeyCode::Esc => {
                self.step = IntegrationStep::SelectTarget;
                true
            }
            _ => false,
        }
    }

    fn toggle_action_at_cursor(&mut self) {
        match ACTION_ROWS[self.action_cursor] {
            ActionRow::WriteShim => self.write_shim = !self.write_shim,
            ActionRow::RegisterMcp => self.register_mcp = !self.register_mcp,
            ActionRow::OverwriteShim => self.overwrite_shim = !self.overwrite_shim,
        }
    }

    fn any_action_selected(&self) -> bool {
        self.write_shim || self.register_mcp
    }

    fn handle_confirm(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Enter | KeyCode::Char('y') => {
                self.step = IntegrationStep::Complete;
                true
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                self.step = IntegrationStep::SelectActions;
                true
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.cancelled = true;
                self.step = IntegrationStep::Complete;
                true
            }
            _ => false,
        }
    }

    /// If the state machine completed without cancelling, produce the plan.
    pub fn finalize(&self) -> Option<IntegrationPlan> {
        if self.cancelled || self.step != IntegrationStep::Complete {
            return None;
        }
        if !self.any_action_selected() {
            return None;
        }
        Some(IntegrationPlan {
            target: self.target,
            write_shim: self.write_shim,
            register_mcp: self.register_mcp,
            overwrite_shim: self.overwrite_shim,
        })
    }
}

/// Default action flags for a given current integration state. Partial state
/// defaults to "finish MCP registration"; Complete state defaults to nothing
/// selected (user must explicitly opt in to regen). Absent or a different
/// target defaults to "write shim and register mcp".
fn default_actions_for(current: &AgentIntegration, target: AgentTargetKind) -> (bool, bool) {
    match current {
        AgentIntegration::Complete { target: t } if *t == target => (false, false),
        AgentIntegration::Partial { target: t } if *t == target => (false, true),
        _ => (true, true),
    }
}

/// Run the integration sub-wizard until Complete or cancellation.
pub fn run_integration_wizard_loop(
    theme: Theme,
    current: AgentIntegration,
    detected_targets: Vec<AgentTargetKind>,
) -> anyhow::Result<IntegrationWizardOutcome> {
    let mut terminal = enter_tui()?;
    let mut state = IntegrationWizardState::new(current, detected_targets);
    let result = render_loop(&mut terminal, &mut state, &theme);
    leave_tui(&mut terminal)?;
    result?;
    if state.cancelled {
        Ok(IntegrationWizardOutcome::Cancelled)
    } else if let Some(plan) = state.finalize() {
        Ok(IntegrationWizardOutcome::Completed { plan })
    } else {
        Ok(IntegrationWizardOutcome::Cancelled)
    }
}

fn render_loop(
    terminal: &mut WizardTerminal,
    state: &mut IntegrationWizardState,
    theme: &Theme,
) -> anyhow::Result<()> {
    use std::time::Duration;
    while state.step != IntegrationStep::Complete {
        terminal.draw(|frame| draw(frame, state, theme))?;
        if let Some((code, mods)) = poll_key(Duration::from_millis(250))? {
            state.handle_key(code, mods);
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, state: &IntegrationWizardState, theme: &Theme) {
    let size = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(6),    // body
            Constraint::Length(3), // hints
        ])
        .split(size);

    let title = Paragraph::new(Line::from(Span::styled(
        match state.step {
            IntegrationStep::SelectTarget => " synrepo integrate — step 1/3: target ",
            IntegrationStep::SelectActions => " synrepo integrate — step 2/3: actions ",
            IntegrationStep::Confirm => " synrepo integrate — step 3/3: confirm ",
            IntegrationStep::Complete => " synrepo integrate — done ",
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
        IntegrationStep::SelectTarget => draw_target_step(frame, outer[1], state, theme),
        IntegrationStep::SelectActions => draw_actions_step(frame, outer[1], state, theme),
        IntegrationStep::Confirm => draw_confirm_step(frame, outer[1], state, theme),
        IntegrationStep::Complete => {}
    }

    let hint = match state.step {
        IntegrationStep::SelectTarget => " ↑/↓ move  Enter select  Esc cancel ",
        IntegrationStep::SelectActions => " ↑/↓ move  Space toggle  Enter continue  Esc back ",
        IntegrationStep::Confirm => " Enter apply  b back  Ctrl-C abort ",
        IntegrationStep::Complete => "",
    };
    let footer = Paragraph::new(Span::styled(hint, theme.muted_style())).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );
    frame.render_widget(footer, outer[2]);
}

fn draw_target_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &IntegrationWizardState,
    theme: &Theme,
) {
    let configured = state.current.target();
    let items: Vec<ListItem> = WIZARD_TARGETS
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let selected = i == state.target_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let label = if Some(*t) == configured {
                format!("{} (configured)", target_label(*t))
            } else if state.detected_targets.contains(t) {
                format!("{} (detected)", target_label(*t))
            } else {
                target_label(*t).to_string()
            };
            let style = if selected {
                theme.agent_style()
            } else if Some(*t) == configured {
                theme.healthy_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(format!("{marker}{label}"), style)))
        })
        .collect();
    let block = Block::default()
        .title(" target ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}

fn draw_actions_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &IntegrationWizardState,
    theme: &Theme,
) {
    let items: Vec<ListItem> = ACTION_ROWS
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let checked = match row {
                ActionRow::WriteShim => state.write_shim,
                ActionRow::RegisterMcp => state.register_mcp,
                ActionRow::OverwriteShim => state.overwrite_shim,
            };
            let check = if checked { "[x]" } else { "[ ]" };
            let label = match row {
                ActionRow::WriteShim => "Write or update the agent shim",
                ActionRow::RegisterMcp => "Register the synrepo MCP server",
                ActionRow::OverwriteShim => {
                    "Overwrite an existing shim if its content differs (regen)"
                }
            };
            let selected = i == state.action_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme.agent_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{marker}{check} {label}"),
                style,
            )))
        })
        .collect();
    let block = Block::default()
        .title(" actions ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}

fn draw_confirm_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &IntegrationWizardState,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("Target: {}", target_label(state.target)),
        theme.base_style(),
    )));
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "The wizard will run the following actions:",
        theme.base_style(),
    )));
    let mut step = 1usize;
    if state.write_shim {
        let suffix = if state.overwrite_shim {
            " (may overwrite existing shim if content differs)"
        } else {
            " (skip if shim already up to date)"
        };
        lines.push(Line::from(Span::styled(
            format!("  {step}. Write or update the agent shim{suffix}"),
            theme.base_style(),
        )));
        step += 1;
    }
    if state.register_mcp {
        lines.push(Line::from(Span::styled(
            format!("  {step}. Register the synrepo MCP server"),
            theme.base_style(),
        )));
    }
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "No files have been written yet. Press Enter to apply or b to go back.",
        theme.muted_style(),
    )));

    let block = Block::default()
        .title(" confirm ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(state: &mut IntegrationWizardState, code: KeyCode) {
        state.handle_key(code, KeyModifiers::empty());
    }

    fn absent() -> AgentIntegration {
        AgentIntegration::Absent
    }
    fn partial(t: AgentTargetKind) -> AgentIntegration {
        AgentIntegration::Partial { target: t }
    }
    fn complete(t: AgentTargetKind) -> AgentIntegration {
        AgentIntegration::Complete { target: t }
    }

    #[test]
    fn absent_seeds_write_shim_and_register_mcp_on() {
        let s = IntegrationWizardState::new(absent(), vec![]);
        assert_eq!(s.step, IntegrationStep::SelectTarget);
        assert!(s.write_shim);
        assert!(s.register_mcp);
        assert!(!s.overwrite_shim);
    }

    #[test]
    fn partial_seeds_only_register_mcp_on() {
        let s = IntegrationWizardState::new(partial(AgentTargetKind::Claude), vec![]);
        assert_eq!(s.target, AgentTargetKind::Claude);
        assert!(!s.write_shim);
        assert!(s.register_mcp);
        assert!(!s.overwrite_shim);
    }

    #[test]
    fn complete_seeds_all_off_and_forces_explicit_opt_in() {
        // Never overwrite an existing fully-configured integration without an
        // explicit opt-in inside the wizard.
        let s = IntegrationWizardState::new(complete(AgentTargetKind::Cursor), vec![]);
        assert_eq!(s.target, AgentTargetKind::Cursor);
        assert!(!s.write_shim);
        assert!(!s.register_mcp);
        assert!(!s.overwrite_shim);
    }

    #[test]
    fn changing_target_reseeds_defaults() {
        let mut s = IntegrationWizardState::new(complete(AgentTargetKind::Claude), vec![]);
        // Target 0 is Claude → Complete; defaults off.
        assert_eq!(s.target_cursor, 0);
        assert!(!s.write_shim);
        // Press Down to move to Cursor (target_cursor=1, not Complete anymore).
        press(&mut s, KeyCode::Down);
        assert_eq!(s.target_cursor, 1);
        assert!(
            s.write_shim,
            "new target with absent integration seeds write_shim=on"
        );
        assert!(s.register_mcp);
    }

    #[test]
    fn happy_path_writes_shim_and_registers_mcp() {
        let mut s = IntegrationWizardState::new(absent(), vec![AgentTargetKind::Claude]);
        press(&mut s, KeyCode::Enter); // target → actions
        assert_eq!(s.step, IntegrationStep::SelectActions);
        press(&mut s, KeyCode::Enter); // actions → confirm
        assert_eq!(s.step, IntegrationStep::Confirm);
        press(&mut s, KeyCode::Enter); // confirm → complete
        let plan = s.finalize().expect("plan");
        assert_eq!(plan.target, AgentTargetKind::Claude);
        assert!(plan.write_shim);
        assert!(plan.register_mcp);
        assert!(!plan.overwrite_shim);
    }

    #[test]
    fn overwrite_toggle_requires_explicit_space_keypress() {
        let mut s = IntegrationWizardState::new(complete(AgentTargetKind::Claude), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
                                       // Defaults are all off; press Enter should refuse to advance (no-op).
        press(&mut s, KeyCode::Enter);
        assert_eq!(
            s.step,
            IntegrationStep::SelectActions,
            "enter with no actions selected must not advance",
        );
        // Navigate to the overwrite row and toggle it on; then Enter should
        // still refuse (overwrite alone doesn't imply write_shim).
        press(&mut s, KeyCode::Down); // cursor → register_mcp
        press(&mut s, KeyCode::Down); // cursor → overwrite_shim
        press(&mut s, KeyCode::Char(' '));
        assert!(s.overwrite_shim);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, IntegrationStep::SelectActions);
        // Now toggle write_shim on; Enter should advance.
        press(&mut s, KeyCode::Up);
        press(&mut s, KeyCode::Up); // cursor → write_shim
        press(&mut s, KeyCode::Char(' '));
        assert!(s.write_shim);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, IntegrationStep::Confirm);
    }

    #[test]
    fn esc_at_target_cancels_without_plan() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn esc_at_actions_returns_to_target_step() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
        press(&mut s, KeyCode::Esc);
        assert_eq!(s.step, IntegrationStep::SelectTarget);
        assert!(!s.cancelled);
    }

    #[test]
    fn ctrl_c_at_confirm_cancels() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
        press(&mut s, KeyCode::Enter); // actions → confirm
        s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn back_at_confirm_returns_to_actions() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, IntegrationStep::Confirm);
        press(&mut s, KeyCode::Char('b'));
        assert_eq!(s.step, IntegrationStep::SelectActions);
    }

    #[test]
    fn completion_with_no_actions_yields_no_plan() {
        let mut s = IntegrationWizardState::new(complete(AgentTargetKind::Claude), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
                                       // Never advance further; simulate the state machine exiting with
                                       // neither flag set.
        s.step = IntegrationStep::Complete;
        assert!(s.finalize().is_none());
    }
}
