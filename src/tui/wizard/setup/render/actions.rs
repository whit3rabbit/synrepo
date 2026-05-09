use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::super::{SetupActionRow, SetupWizardState, SETUP_ACTION_ROWS};
use crate::tui::theme::Theme;
use crate::tui::wizard::{target_artifact_label, target_label, target_tier, AgentTargetTier};

pub(super) fn draw_actions_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let items: Vec<ListItem> = SETUP_ACTION_ROWS
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let checked = action_checked(state, *row);
            let check = if checked { "[x]" } else { "[ ]" };
            let label = action_label(state, *row);
            let selected = i == state.action_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme.agent_style()
            } else if action_enabled(state, *row) {
                theme.base_style()
            } else {
                theme.muted_style()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{marker}{check} {label}"),
                style,
            )))
        })
        .collect();
    let block = Block::default()
        .title(" repo-local actions ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}

fn action_checked(state: &SetupWizardState, row: SetupActionRow) -> bool {
    match row {
        SetupActionRow::AddRootGitignore => state.add_root_gitignore,
        SetupActionRow::WriteAgentShim => state.write_agent_shim,
        SetupActionRow::RegisterMcp => state.register_mcp,
        SetupActionRow::InstallAgentHooks => state.install_agent_hooks,
    }
}

fn action_enabled(state: &SetupWizardState, row: SetupActionRow) -> bool {
    match row {
        SetupActionRow::AddRootGitignore => true,
        SetupActionRow::WriteAgentShim => state.target.is_some(),
        SetupActionRow::RegisterMcp => state.target_can_register_mcp(),
        SetupActionRow::InstallAgentHooks => state.target_supports_hooks(),
    }
}

fn action_label(state: &SetupWizardState, row: SetupActionRow) -> String {
    match row {
        SetupActionRow::AddRootGitignore => {
            if state.root_gitignore_present {
                "Root .gitignore already contains .synrepo/".to_string()
            } else {
                "Add .synrepo/ to root .gitignore".to_string()
            }
        }
        SetupActionRow::WriteAgentShim => match state.target {
            Some(target) => format!(
                "Write or update {} {}",
                target_label(target),
                target_artifact_label(target)
            ),
            None => "Write agent skill/instructions (select a target first)".to_string(),
        },
        SetupActionRow::RegisterMcp => match state.target {
            Some(target) if target_tier(target) == AgentTargetTier::Automated => {
                format!(
                    "Register repo-local MCP server for {}",
                    target_label(target)
                )
            }
            Some(target) => format!("MCP registration is manual for {}", target_label(target)),
            None => "Register repo-local MCP server (select a target first)".to_string(),
        },
        SetupActionRow::InstallAgentHooks => match state.target {
            Some(target) if state.target_supports_hooks() => {
                format!("Install local nudge hooks for {}", target_label(target))
            }
            Some(target) => format!(
                "Local nudge hooks are not supported for {}",
                target_label(target)
            ),
            None => "Install local nudge hooks (select Codex or Claude first)".to_string(),
        },
    }
}
