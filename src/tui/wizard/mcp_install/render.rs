//! Repo-local MCP install picker rendering.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::state::{McpInstallStep, McpInstallWizardState};
use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::{enter_tui, leave_tui, WizardTerminal};

/// Run the repo-local MCP install picker until completion or cancellation.
pub fn run_mcp_install_wizard_loop(
    theme: Theme,
    repo_root: &std::path::Path,
    rows: Vec<crate::tui::mcp_status::McpStatusRow>,
    detected_targets: Vec<crate::bootstrap::runtime_probe::AgentTargetKind>,
) -> anyhow::Result<super::McpInstallWizardOutcome> {
    let mut terminal = enter_tui()?;
    let mut state = super::state::McpInstallWizardState::new(repo_root, rows, detected_targets);
    let result = render_loop(&mut terminal, &mut state, &theme);
    leave_tui(&mut terminal)?;
    result?;
    if state.cancelled {
        Ok(super::McpInstallWizardOutcome::Cancelled)
    } else if let Some(plan) = state.finalize() {
        Ok(super::McpInstallWizardOutcome::Completed { plan })
    } else {
        Ok(super::McpInstallWizardOutcome::Cancelled)
    }
}

fn render_loop(
    terminal: &mut WizardTerminal,
    state: &mut super::state::McpInstallWizardState,
    theme: &Theme,
) -> anyhow::Result<()> {
    use std::time::Duration;
    while state.step != McpInstallStep::Complete {
        terminal.draw(|frame| draw(frame, state, theme))?;
        if let Some((code, mods)) = poll_key(Duration::from_millis(250))? {
            state.handle_key(code, mods);
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, state: &McpInstallWizardState, theme: &Theme) {
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
            McpInstallStep::SelectTarget => " synrepo mcp install: step 1/2 target ",
            McpInstallStep::Confirm => " synrepo mcp install: step 2/2 confirm ",
            McpInstallStep::Complete => " synrepo mcp install: done ",
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
        McpInstallStep::SelectTarget => draw_target_step(frame, outer[1], state, theme),
        McpInstallStep::Confirm => draw_confirm_step(frame, outer[1], state, theme),
        McpInstallStep::Complete => {}
    }

    let hint = match state.step {
        McpInstallStep::SelectTarget => " Up/Down move  Enter select  Esc cancel ",
        McpInstallStep::Confirm => " Enter apply  b back  Ctrl-C abort ",
        McpInstallStep::Complete => "",
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
    state: &McpInstallWizardState,
    theme: &Theme,
) {
    if state.rows().is_empty() {
        let lines = vec![
            Line::from(""),
            Line::from("  no local MCP-capable targets found."),
            Line::from("  Press Esc to cancel."),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .block(target_block(theme))
                .style(theme.muted_style()),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = state
        .rows()
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = i == state.target_cursor;
            let marker = if selected { "> " } else { "  " };
            let detected = if row.detected { " detected" } else { "" };
            let path = row
                .config_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            let label = format!(
                "{}{} [{}] scope:{} source:{} path:{}",
                marker,
                row.agent,
                row.status.as_str(),
                row.scope.as_str(),
                row.source,
                path
            );
            let style = if selected {
                theme.agent_style()
            } else if row.status == crate::tui::mcp_status::McpStatus::Registered {
                theme.healthy_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{label}{detected}"),
                style,
            )))
        })
        .collect();
    frame.render_widget(List::new(items).block(target_block(theme)), area);
}

fn draw_confirm_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &McpInstallWizardState,
    theme: &Theme,
) {
    let Some(row) = state.selected_row() else {
        return;
    };
    let path = row
        .config_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    let lines = vec![
        Line::from(Span::styled(
            format!("Target: {}", row.agent),
            theme.base_style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The picker will install repo-local MCP only:",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "  1. Back up the target project MCP config if needed",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "  2. Register command: synrepo mcp --repo .",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "  3. Leave agent skills and instructions untouched",
            theme.base_style(),
        )),
        Line::from(""),
        Line::from(Span::styled(format!("Config: {path}"), theme.muted_style())),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" confirm ")
                    .borders(Borders::ALL)
                    .border_style(theme.border_style()),
            )
            .style(theme.base_style()),
        area,
    );
}

fn target_block(theme: &Theme) -> Block<'_> {
    Block::default()
        .title(" local MCP target ")
        .borders(Borders::ALL)
        .border_style(theme.border_style())
}
