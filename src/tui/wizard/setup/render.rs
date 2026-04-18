//! Setup wizard rendering.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::state::{SetupStep, SetupWizardState, WIZARD_TARGETS};
use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::{enter_tui, leave_tui, target_label, WizardTerminal};

/// Run the setup wizard until Complete or cancellation.
pub fn run_setup_wizard_loop(
    theme: Theme,
    default_mode: crate::config::Mode,
    detected_targets: Vec<crate::bootstrap::runtime_probe::AgentTargetKind>,
) -> anyhow::Result<super::SetupWizardOutcome> {
    let mut terminal = enter_tui()?;
    let mut state = SetupWizardState::new(default_mode, detected_targets);
    let result = render_loop(&mut terminal, &mut state, &theme);
    leave_tui(&mut terminal)?;
    result?;
    if state.cancelled {
        Ok(super::SetupWizardOutcome::Cancelled)
    } else if let Some(plan) = state.finalize() {
        Ok(super::SetupWizardOutcome::Completed { plan })
    } else {
        Ok(super::SetupWizardOutcome::Cancelled)
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
            SetupStep::Splash => " synrepo setup — step 1/4: welcome ",
            SetupStep::SelectMode => " synrepo setup — step 2/4: graph mode ",
            SetupStep::SelectTarget => " synrepo setup — step 3/4: agent integration ",
            SetupStep::Confirm => " synrepo setup — step 4/4: confirm ",
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
        SetupStep::Splash => draw_splash_step(frame, outer[1], theme),
        SetupStep::SelectMode => draw_mode_step(frame, outer[1], state, theme),
        SetupStep::SelectTarget => draw_target_step(frame, outer[1], state, theme),
        SetupStep::Confirm => draw_confirm_step(frame, outer[1], state, theme),
        SetupStep::Complete => {}
    }

    let hint = match state.step {
        SetupStep::Splash => " Enter continue  Esc exit ",
        SetupStep::SelectMode | SetupStep::SelectTarget => " ↑/↓ move  Enter select  Esc cancel ",
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

fn draw_splash_step(frame: &mut ratatui::Frame, area: Rect, theme: &Theme) {
    let lines: Vec<Line> = vec![
        Line::from(Span::styled("Welcome to synrepo.", theme.agent_style())),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "synrepo builds a local, inspectable graph of your repository so",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "agents can answer code questions without re-scanning every file.",
            theme.base_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Estimated setup time: under a minute on most repos.",
            theme.muted_style(),
        )),
        Line::from(Span::styled(
            "Nothing leaves your machine. All state lives under .synrepo/.",
            theme.muted_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Press Enter to continue, or Esc to exit without changes.",
            theme.base_style(),
        )),
    ];
    let block = Block::default()
        .title(" welcome ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(Paragraph::new(lines).block(block), area);
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
