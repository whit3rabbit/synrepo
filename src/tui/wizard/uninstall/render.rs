//! Uninstall wizard rendering.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::state::{UninstallActionKind, UninstallStep, UninstallWizardState};
use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::{enter_tui, leave_tui, WizardTerminal};

/// Run the uninstall wizard until Complete or cancellation.
pub fn run_uninstall_wizard_loop(
    theme: Theme,
    installed: &[UninstallActionKind],
    preserved: &[std::path::PathBuf],
) -> anyhow::Result<super::UninstallWizardOutcome> {
    let mut terminal = enter_tui()?;
    let mut state = UninstallWizardState::new(installed, preserved);
    let result = render_loop(&mut terminal, &mut state, &theme);
    leave_tui(&mut terminal)?;
    result?;
    if state.cancelled {
        Ok(super::UninstallWizardOutcome::Cancelled)
    } else if let Some(plan) = state.finalize() {
        Ok(super::UninstallWizardOutcome::Completed { plan })
    } else {
        Ok(super::UninstallWizardOutcome::Cancelled)
    }
}

fn render_loop(
    terminal: &mut WizardTerminal,
    state: &mut UninstallWizardState,
    theme: &Theme,
) -> anyhow::Result<()> {
    use std::time::Duration;
    while state.step != UninstallStep::Complete {
        terminal.draw(|frame| draw(frame, state, theme))?;
        if let Some((code, mods)) = poll_key(Duration::from_millis(250))? {
            state.handle_key(code, mods);
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, state: &UninstallWizardState, theme: &Theme) {
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
            UninstallStep::Select => " synrepo remove — select artifacts ",
            UninstallStep::Confirm => " synrepo remove — confirm ",
            UninstallStep::Complete => " synrepo remove — done ",
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
        UninstallStep::Select => draw_select(frame, outer[1], state, theme),
        UninstallStep::Confirm => draw_confirm(frame, outer[1], state, theme),
        UninstallStep::Complete => {}
    }

    let hint = match state.step {
        UninstallStep::Select => " ↑/↓ move  Space toggle  Enter continue  Esc cancel ",
        UninstallStep::Confirm => " Enter apply  b back  Ctrl-C abort ",
        UninstallStep::Complete => "",
    };
    let footer = Paragraph::new(Span::styled(hint, theme.muted_style())).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );
    frame.render_widget(footer, outer[2]);
}

fn draw_select(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &UninstallWizardState,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Checked rows will be removed. Space toggles; destructive rows start off.",
        theme.muted_style(),
    )));
    if !state.preserved.is_empty() {
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(Span::styled(
            "Preserved (never removed):",
            theme.muted_style(),
        )));
        for p in &state.preserved {
            lines.push(Line::from(Span::styled(
                format!("  {}", p.display()),
                theme.muted_style(),
            )));
        }
    }

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

    let context_height = (lines.len() as u16).saturating_add(2);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(context_height), Constraint::Min(2)])
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
                .title(" artifacts "),
        ),
        split[1],
    );
}

fn draw_confirm(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &UninstallWizardState,
    theme: &Theme,
) {
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
