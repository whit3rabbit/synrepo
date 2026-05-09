//! Integrations tab widget.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Widget};

use crate::tui::agent_integrations::AgentInstallDisplayRow;
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// Render active-project agent integration status.
pub struct IntegrationsTabWidget<'a> {
    /// Rows resolved for the active project.
    pub rows: &'a [AgentInstallDisplayRow],
    /// Selected row index, clamped at render time.
    pub selected: usize,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for IntegrationsTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" integrations ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        if self.rows.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from("  no agent integrations detected."),
                Line::from(""),
                Line::from("  press [i] to install project integration,"),
                Line::from("  or run: synrepo setup <tool> --project"),
            ];
            Paragraph::new(lines)
                .block(block)
                .style(self.theme.muted_style())
                .render(area, buf);
            return;
        }
        let header = Row::new(vec![
            Cell::from("agent"),
            Cell::from("overall"),
            Cell::from("context"),
            Cell::from("mcp"),
            Cell::from("hooks"),
            Cell::from("next"),
        ])
        .style(self.theme.muted_style());
        let selected = self.selected.min(self.rows.len().saturating_sub(1));
        let rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(idx, row)| table_row(row, idx == selected, self.theme));
        Table::new(
            rows,
            [
                Constraint::Length(20),
                Constraint::Length(10),
                Constraint::Percentage(24),
                Constraint::Percentage(24),
                Constraint::Percentage(18),
                Constraint::Percentage(20),
            ],
        )
        .header(header)
        .block(block)
        .style(self.theme.base_style())
        .render(area, buf);
    }
}

fn table_row(row: &AgentInstallDisplayRow, selected: bool, theme: &Theme) -> Row<'static> {
    let marker = if selected { "> " } else { "  " };
    let style = if selected {
        theme.agent_style()
    } else {
        theme.base_style()
    };
    Row::new(vec![
        Cell::from(Span::styled(format!("{marker}{}", row.agent), style)),
        Cell::from(Line::from(vec![severity_span(
            row.overall_label,
            row.overall_severity,
            theme,
        )])),
        Cell::from(Line::from(vec![severity_span(
            &row.context,
            row.context_severity,
            theme,
        )])),
        Cell::from(Line::from(vec![severity_span(
            &row.mcp,
            row.mcp_severity,
            theme,
        )])),
        Cell::from(Line::from(vec![severity_span(
            &row.hooks,
            row.hooks_severity,
            theme,
        )])),
        Cell::from(Span::styled(row.next_action.clone(), theme.muted_style())),
    ])
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;

    use super::*;
    use crate::tui::probe::Severity;

    fn render_text(width: u16) -> String {
        render_text_selected(width, 0)
    }

    fn render_text_selected(width: u16, selected: usize) -> String {
        let area = Rect::new(0, 0, width, 8);
        let mut buf = Buffer::empty(area);
        let rows = vec![
            display_row("codex", "Codex CLI"),
            display_row("claude", "Claude Code"),
        ];
        IntegrationsTabWidget {
            rows: &rows,
            selected,
            theme: &Theme::plain(),
        }
        .render(area, &mut buf);
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn display_row(tool: &str, agent: &str) -> AgentInstallDisplayRow {
        AgentInstallDisplayRow {
            tool: tool.to_string(),
            agent: agent.to_string(),
            overall_label: "complete",
            overall_severity: Severity::Healthy,
            context: "skill installed project agent-config owned .agents/skills/synrepo/SKILL.md"
                .to_string(),
            context_severity: Severity::Healthy,
            mcp: "mcp installed project agent-config owned .codex/config.toml".to_string(),
            mcp_severity: Severity::Healthy,
            hooks: "missing_optional optional .codex/hooks.json".to_string(),
            hooks_severity: Severity::Healthy,
            next_action: "optional: synrepo setup codex --agent-hooks".to_string(),
        }
    }

    #[test]
    fn integrations_tab_renders_columns_at_normal_width() {
        let text = render_text(120);
        assert!(text.contains("integrations"));
        assert!(text.contains("agent"));
        assert!(text.contains("overall"));
        assert!(text.contains("context"));
        assert!(text.contains("mcp"));
        assert!(text.contains("hooks"));
        assert!(text.contains("Codex CLI"));
    }

    #[test]
    fn integrations_tab_marks_selected_row() {
        let text = render_text_selected(120, 1);
        assert!(text.contains("> Claude Code"));
    }

    #[test]
    fn integrations_tab_clips_cleanly_at_narrow_width() {
        let text = render_text(60);
        assert!(text.contains("integrations"));
        assert!(text.contains("agent"));
        assert!(text.contains("overall"));
        for line in text.lines() {
            assert_eq!(line.chars().count(), 60);
        }
    }
}
