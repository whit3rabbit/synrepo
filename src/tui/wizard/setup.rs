//! Setup wizard: guided first-run flow for uninitialized repos.
//!
//! Three steps — graph mode, agent target, confirm — and then the wizard
//! returns a [`SetupPlan`] the bin-side dispatcher executes. Cancellation at
//! any point before Confirm guarantees zero writes. The state machine is
//! deliberately trivial (no `async`, no shared state) so the unit tests can
//! exercise it by driving `handle_key` with crafted key events and asserting
//! on the resulting plan.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::config::Mode;
use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::{enter_tui, leave_tui, target_label, WizardTerminal};

/// Plan produced by a completed setup wizard. Executed by the bin-side
/// dispatcher after the TUI alternate-screen has been torn down.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupPlan {
    /// Config mode to write into `.synrepo/config.toml`.
    pub mode: Mode,
    /// Optional agent-integration target. `None` means the user chose "skip".
    pub target: Option<AgentTargetKind>,
    /// Whether to run a reconcile pass after init finishes. Always `true` for
    /// v1 so the first-run experience is complete; kept as a field so future
    /// "fast-path" flows can opt out without widening the plan shape.
    pub reconcile_after: bool,
}

/// Outcome returned by the setup wizard. Distinct from the dashboard's
/// [`crate::tui::TuiOutcome`] so the bin-side dispatcher can pattern-match on
/// the plan without reading an `Option` through `TuiOutcome`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupWizardOutcome {
    /// Stdout is not a TTY; the wizard was not entered.
    NonTty,
    /// Operator cancelled before the confirm step. No writes performed.
    Cancelled,
    /// Operator confirmed; caller must execute `plan`.
    Completed {
        /// Plan to execute.
        plan: SetupPlan,
    },
}

/// Steps the setup wizard walks through in order. The `Complete` step is the
/// terminal state that causes the render loop to exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupStep {
    /// Pick graph mode (auto or curated).
    SelectMode,
    /// Pick agent-integration target or "skip".
    SelectTarget,
    /// Review the plan and press Enter to apply.
    Confirm,
    /// Render loop should exit; outcome is already captured.
    Complete,
}

/// V1 target roster offered by the wizard. Wider `AgentTool` variants
/// (Gemini, Goose, Kiro, etc.) stay behind `synrepo agent-setup <tool>` for
/// now; the wizard only offers the observationally-detectable five.
pub const WIZARD_TARGETS: &[AgentTargetKind] = &[
    AgentTargetKind::Claude,
    AgentTargetKind::Cursor,
    AgentTargetKind::Codex,
    AgentTargetKind::Copilot,
    AgentTargetKind::Windsurf,
];

/// State machine driving the setup wizard. Tests drive this struct directly
/// via [`SetupWizardState::handle_key`] and assert on `finalize()` /
/// `cancelled` / `step`.
#[derive(Clone, Debug)]
pub struct SetupWizardState {
    /// Current step.
    pub step: SetupStep,
    /// Cursor index in the mode list: 0 = Auto, 1 = Curated.
    pub mode_cursor: usize,
    /// Cursor index in the target list: 0..N for targets, N for "Skip".
    pub target_cursor: usize,
    /// Committed mode (set on Enter at `SelectMode`).
    pub mode: Mode,
    /// Committed target (set on Enter at `SelectTarget`). `None` means skip.
    pub target: Option<AgentTargetKind>,
    /// Deterministic ordered list of agent targets detected from repo /
    /// `$HOME` hints. Used to pre-select the target cursor.
    pub detected_targets: Vec<AgentTargetKind>,
    /// True when the operator pressed Esc / Ctrl-C / q before Confirm.
    pub cancelled: bool,
}

impl SetupWizardState {
    /// Build a fresh state. `default_mode` seeds the mode cursor; a caller
    /// that detected concept directories passes `Mode::Curated`.
    /// `detected_targets` seeds the target cursor to the first detected
    /// target, falling back to position 0 (the first target in
    /// [`WIZARD_TARGETS`]).
    pub fn new(default_mode: Mode, detected_targets: Vec<AgentTargetKind>) -> Self {
        let mode_cursor = match default_mode {
            Mode::Auto => 0,
            Mode::Curated => 1,
        };
        let target_cursor = detected_targets
            .first()
            .and_then(|t| WIZARD_TARGETS.iter().position(|wt| wt == t))
            .unwrap_or(0);
        Self {
            step: SetupStep::SelectMode,
            mode_cursor,
            target_cursor,
            mode: default_mode,
            target: None,
            detected_targets,
            cancelled: false,
        }
    }

    /// Handle a key event; returns `true` if the loop should redraw. Pressing
    /// Esc / Ctrl-C / q at any step before `Confirm` cancels the wizard. At
    /// `Confirm`, Esc / b steps back rather than cancelling; Ctrl-C still
    /// cancels.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let is_quit = matches!(code, KeyCode::Esc | KeyCode::Char('q'))
            || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL));

        match self.step {
            SetupStep::SelectMode => {
                if is_quit {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.mode_cursor = self.mode_cursor.saturating_sub(1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.mode_cursor < 1 {
                            self.mode_cursor += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        self.mode = if self.mode_cursor == 0 {
                            Mode::Auto
                        } else {
                            Mode::Curated
                        };
                        self.step = SetupStep::SelectTarget;
                        true
                    }
                    _ => false,
                }
            }
            SetupStep::SelectTarget => {
                if is_quit {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                let max = WIZARD_TARGETS.len(); // N = skip position
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.target_cursor = self.target_cursor.saturating_sub(1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.target_cursor < max {
                            self.target_cursor += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        self.target = WIZARD_TARGETS.get(self.target_cursor).copied();
                        self.step = SetupStep::Confirm;
                        true
                    }
                    _ => false,
                }
            }
            SetupStep::Confirm => match code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.step = SetupStep::Complete;
                    true
                }
                KeyCode::Esc | KeyCode::Char('b') => {
                    // Back to target selection; do not cancel.
                    self.step = SetupStep::SelectTarget;
                    true
                }
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    // Explicit abort at confirm still cancels.
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    true
                }
                _ => false,
            },
            SetupStep::Complete => false,
        }
    }

    /// If the state machine completed without cancelling, produce the plan.
    pub fn finalize(&self) -> Option<SetupPlan> {
        if self.cancelled || self.step != SetupStep::Complete {
            return None;
        }
        Some(SetupPlan {
            mode: self.mode,
            target: self.target,
            reconcile_after: true,
        })
    }
}

/// Run the setup wizard until Complete or cancellation.
pub fn run_setup_wizard_loop(
    theme: Theme,
    default_mode: Mode,
    detected_targets: Vec<AgentTargetKind>,
) -> anyhow::Result<SetupWizardOutcome> {
    let mut terminal = enter_tui()?;
    let mut state = SetupWizardState::new(default_mode, detected_targets);
    let result = render_loop(&mut terminal, &mut state, &theme);
    leave_tui(&mut terminal)?;
    result?;
    if state.cancelled {
        Ok(SetupWizardOutcome::Cancelled)
    } else if let Some(plan) = state.finalize() {
        Ok(SetupWizardOutcome::Completed { plan })
    } else {
        Ok(SetupWizardOutcome::Cancelled)
    }
}

fn render_loop(
    terminal: &mut WizardTerminal,
    state: &mut SetupWizardState,
    theme: &Theme,
) -> anyhow::Result<()> {
    use std::time::Duration;
    while state.step != SetupStep::Complete {
        terminal.draw(|frame| draw(frame, state, theme))?;
        if let Some((code, mods)) = poll_key(Duration::from_millis(250))? {
            state.handle_key(code, mods);
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, state: &SetupWizardState, theme: &Theme) {
    let size = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(6),    // body
            Constraint::Length(3), // footer hints
        ])
        .split(size);

    let title = Paragraph::new(Line::from(Span::styled(
        match state.step {
            SetupStep::SelectMode => " synrepo setup — step 1/3: graph mode ",
            SetupStep::SelectTarget => " synrepo setup — step 2/3: agent integration ",
            SetupStep::Confirm => " synrepo setup — step 3/3: confirm ",
            SetupStep::Complete => " synrepo setup — done ",
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
        SetupStep::SelectMode => draw_mode_step(frame, outer[1], state, theme),
        SetupStep::SelectTarget => draw_target_step(frame, outer[1], state, theme),
        SetupStep::Confirm => draw_confirm_step(frame, outer[1], state, theme),
        SetupStep::Complete => {}
    }

    let hint = match state.step {
        SetupStep::SelectMode | SetupStep::SelectTarget => {
            " ↑/↓ move  Enter select  Esc cancel "
        }
        SetupStep::Confirm => " Enter apply  b back  Ctrl-C abort ",
        SetupStep::Complete => "",
    };
    let footer = Paragraph::new(Span::styled(hint, theme.muted_style())).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );
    frame.render_widget(footer, outer[2]);
}

fn draw_mode_step(frame: &mut ratatui::Frame, area: Rect, state: &SetupWizardState, theme: &Theme) {
    let rows = [
        "Auto — index everything observable (recommended for new repos).",
        "Curated — index only the paths you configure (recommended when docs/ is large).",
    ];
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let selected = i == state.mode_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme.agent_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(format!("{marker}{label}"), style)))
        })
        .collect();
    let block = Block::default()
        .title(" mode ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}

fn draw_target_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let mut rows: Vec<(String, bool)> = WIZARD_TARGETS
        .iter()
        .map(|t| {
            let detected = state.detected_targets.contains(t);
            let label = if detected {
                format!("{} (detected)", target_label(*t))
            } else {
                target_label(*t).to_string()
            };
            (label, detected)
        })
        .collect();
    rows.push(("Skip — I'll set up integration later".to_string(), false));

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, (label, detected))| {
            let selected = i == state.target_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme.agent_style()
            } else if *detected {
                theme.healthy_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(format!("{marker}{label}"), style)))
        })
        .collect();
    let block = Block::default()
        .title(" agent target ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}

fn draw_confirm_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "The wizard will run the following steps:",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        format!("  1. init .synrepo/ in {} mode", state.mode),
        theme.base_style(),
    )));
    match state.target {
        Some(target) => {
            lines.push(Line::from(Span::styled(
                format!("  2. write agent shim for {}", target_label(target)),
                theme.base_style(),
            )));
            lines.push(Line::from(Span::styled(
                format!("  3. register MCP server for {}", target_label(target)),
                theme.base_style(),
            )));
            lines.push(Line::from(Span::styled(
                "  4. run first reconcile pass",
                theme.base_style(),
            )));
        }
        None => {
            lines.push(Line::from(Span::styled(
                "  2. run first reconcile pass (agent integration skipped)",
                theme.base_style(),
            )));
        }
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
    use crossterm::event::{KeyCode, KeyModifiers};

    fn press(state: &mut SetupWizardState, code: KeyCode) {
        state.handle_key(code, KeyModifiers::empty());
    }

    #[test]
    fn happy_path_default_auto_claude_target() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);
        assert_eq!(s.step, SetupStep::SelectMode);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, SetupStep::SelectTarget);
        assert_eq!(s.mode, Mode::Auto);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, SetupStep::Confirm);
        assert_eq!(s.target, Some(AgentTargetKind::Claude));
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, SetupStep::Complete);
        let plan = s.finalize().expect("plan");
        assert_eq!(plan.mode, Mode::Auto);
        assert_eq!(plan.target, Some(AgentTargetKind::Claude));
        assert!(plan.reconcile_after);
    }

    #[test]
    fn select_curated_and_skip_target() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![]);
        press(&mut s, KeyCode::Down);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.mode, Mode::Curated);
        for _ in 0..WIZARD_TARGETS.len() {
            press(&mut s, KeyCode::Down);
        }
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.target, None);
        press(&mut s, KeyCode::Enter);
        let plan = s.finalize().expect("plan");
        assert_eq!(plan.mode, Mode::Curated);
        assert_eq!(plan.target, None);
    }

    #[test]
    fn esc_at_mode_step_cancels_with_no_plan() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![]);
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert_eq!(s.step, SetupStep::Complete);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn esc_at_target_step_cancels_with_no_plan() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![]);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn b_at_confirm_goes_back_to_target_step() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![]);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, SetupStep::Confirm);
        press(&mut s, KeyCode::Char('b'));
        assert_eq!(s.step, SetupStep::SelectTarget);
        assert!(!s.cancelled);
    }

    #[test]
    fn ctrl_c_at_confirm_cancels() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![]);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn detected_target_preselects_cursor_when_available() {
        let s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Codex]);
        assert_eq!(s.target_cursor, 2);
    }

    #[test]
    fn detected_target_absent_from_roster_falls_back_to_zero() {
        let s = SetupWizardState::new(Mode::Curated, vec![]);
        assert_eq!(s.target_cursor, 0);
    }

    #[test]
    fn up_at_top_does_not_underflow() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![]);
        press(&mut s, KeyCode::Up);
        assert_eq!(s.mode_cursor, 0);
    }

    #[test]
    fn down_at_bottom_does_not_overflow() {
        let mut s = SetupWizardState::new(Mode::Auto, vec![]);
        for _ in 0..10 {
            press(&mut s, KeyCode::Down);
        }
        assert_eq!(s.mode_cursor, 1);
    }
}
