//! Setup wizard rendering.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

mod synthesis;

use super::state::{SetupStep, SetupWizardState, WIZARD_TARGETS};
use super::synthesis::SynthesisChoice;
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
            SetupStep::Splash => " synrepo setup — step 1/5: welcome ",
            SetupStep::SelectMode => " synrepo setup — step 2/5: graph mode ",
            SetupStep::SelectTarget => " synrepo setup — step 3/5: agent integration ",
            SetupStep::SelectSynthesis => " synrepo setup — step 4/5: LLM synthesis ",
            SetupStep::SelectLocalPreset => " synrepo setup — step 4a: local LLM preset ",
            SetupStep::EditLocalEndpoint => " synrepo setup — step 4b: local endpoint ",
            SetupStep::Confirm => " synrepo setup — step 5/5: confirm ",
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
        SetupStep::SelectSynthesis => synthesis::draw_synthesis_step(frame, outer[1], state, theme),
        SetupStep::SelectLocalPreset => {
            synthesis::draw_local_preset_step(frame, outer[1], state, theme)
        }
        SetupStep::EditLocalEndpoint => {
            synthesis::draw_local_endpoint_step(frame, outer[1], state, theme)
        }
        SetupStep::Confirm => draw_confirm_step(frame, outer[1], state, theme),
        SetupStep::Complete => {}
    }

    let hint = match state.step {
        SetupStep::Splash => " Enter continue  Esc exit ",
        SetupStep::SelectMode
        | SetupStep::SelectTarget
        | SetupStep::SelectSynthesis
        | SetupStep::SelectLocalPreset => " ↑/↓ move  Enter select  Esc cancel ",
        SetupStep::EditLocalEndpoint => {
            " type URL  Enter accept  Esc back  Ctrl-U clear  Ctrl-C abort "
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
    let mut step_no: usize = 1;
    lines.push(Line::from(Span::styled(
        format!("  {step_no}. init .synrepo/ in {} mode", state.mode),
        theme.base_style(),
    )));
    step_no += 1;
    if let Some(target) = state.target {
        lines.push(Line::from(Span::styled(
            format!("  {step_no}. write agent shim for {}", target_label(target)),
            theme.base_style(),
        )));
        step_no += 1;
        lines.push(Line::from(Span::styled(
            format!(
                "  {step_no}. register MCP server for {}",
                target_label(target)
            ),
            theme.base_style(),
        )));
        step_no += 1;
    }
    match &state.synthesis {
        Some(SynthesisChoice::Cloud(provider)) => {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {step_no}. enable synthesis via {} (set {} in your shell)",
                    provider.config_value(),
                    synthesis::provider_env_var(*provider),
                ),
                theme.base_style(),
            )));
            step_no += 1;
        }
        Some(SynthesisChoice::Local { preset, endpoint }) => {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {step_no}. enable local synthesis ({} at {endpoint})",
                    preset.config_value()
                ),
                theme.base_style(),
            )));
            step_no += 1;
        }
        None => {
            lines.push(Line::from(Span::styled(
                format!("  {step_no}. leave synthesis disabled (no [synthesis] block)"),
                theme.muted_style(),
            )));
            step_no += 1;
        }
    }
    lines.push(Line::from(Span::styled(
        format!("  {step_no}. run first reconcile pass"),
        theme.base_style(),
    )));
    lines.push(Line::from(Span::raw("")));
    if matches!(state.synthesis, Some(SynthesisChoice::Cloud(_))) {
        lines.push(Line::from(Span::styled(
            "Reminder: API keys stay in your shell only. synrepo never writes them to disk.",
            theme.muted_style(),
        )));
        lines.push(Line::from(Span::raw("")));
    }
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
