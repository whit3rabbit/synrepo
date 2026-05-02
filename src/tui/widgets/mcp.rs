//! MCP tab widget.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};

use crate::tui::mcp_status::McpDisplayRow;
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// Render active-project MCP registration status.
pub struct McpTabWidget<'a> {
    /// Rows resolved for the active project.
    pub rows: &'a [McpDisplayRow],
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for McpTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" mcp ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        if self.rows.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from("  no MCP integrations detected."),
                Line::from(""),
                Line::from("  press [i] to launch the integration wizard,"),
                Line::from("  or run: synrepo agent-setup <tool>"),
            ];
            Paragraph::new(lines)
                .block(block)
                .style(self.theme.muted_style())
                .render(area, buf);
            return;
        }
        let items: Vec<ListItem> = self
            .rows
            .iter()
            .map(|row| row_item(row, self.theme))
            .collect();
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

fn row_item(row: &McpDisplayRow, theme: &Theme) -> ListItem<'static> {
    let status = severity_span(row.status_label, row.status_severity, theme);
    ListItem::new(Line::from(vec![
        Span::styled(row.agent_cell.clone(), theme.base_style()),
        Span::styled(" status:", theme.muted_style()),
        status,
        Span::styled(row.scope_cell.clone(), theme.muted_style()),
        Span::styled(row.source_cell.clone(), theme.muted_style()),
        Span::styled(row.path_cell.clone(), theme.muted_style()),
    ]))
}
