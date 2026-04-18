//! Integration wizard rendering.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::state::{ActionRow, IntegrationStep, IntegrationWizardState, ACTION_ROWS};
use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::{
    enter_tui, leave_tui, target_label, target_tier, AgentTargetTier, WizardTerminal,
};

/// Run the integration sub-wizard until Complete or cancellation.
pub fn run_integration_wizard_loop(
    theme: Theme,
    current: crate::bootstrap::runtime_probe::AgentIntegration,
    detected_targets: Vec<crate::bootstrap::runtime_probe::AgentTargetKind>,
) -> anyhow::Result<super::IntegrationWizardOutcome> {
    let mut terminal = enter_tui()?;
    let mut state = super::state::IntegrationWizardState::new(current, detected_targets);
    let result = render_loop(&mut terminal, &mut state, &theme);
    leave_tui(&mut terminal)?;
    result?;
    if state.cancelled {
        Ok(super::IntegrationWizardOutcome::Cancelled)
    } else if let Some(plan) = state.finalize() {
        Ok(super::IntegrationWizardOutcome::Completed { plan })
    } else {
        Ok(super::IntegrationWizardOutcome::Cancelled)
    }
}

fn render_loop(
    terminal: &mut WizardTerminal,
    state: &mut super::state::IntegrationWizardState,
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
    use crate::tui::wizard::setup::WIZARD_TARGETS;
    let configured = state.current.target();
    let items: Vec<ListItem> = WIZARD_TARGETS
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let selected = i == state.target_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let tier = match target_tier(*t) {
                AgentTargetTier::Automated => "automated",
                AgentTargetTier::ShimOnly => "shim-only",
            };
            let status_suffix = if Some(*t) == configured {
                " (configured)"
            } else if state.detected_targets.contains(t) {
                " (detected)"
            } else {
                ""
            };
            let label = format!("{} [{tier}]{status_suffix}", target_label(*t));
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
        if target_tier(state.target) == AgentTargetTier::ShimOnly {
            lines.push(Line::from(Span::styled(
                format!(
                    "     Note: {} uses shim-only integration; \
                     MCP registration will print a manual setup hint instead.",
                    target_label(state.target)
                ),
                theme.muted_style(),
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
