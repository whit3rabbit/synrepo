//! MCP tab widget.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::tui::mcp_status::McpStatusRow;
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// Render active-project MCP registration status.
pub struct McpTabWidget<'a> {
    /// Rows resolved for the active project.
    pub rows: &'a [McpStatusRow],
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for McpTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" mcp ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
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

fn row_item(row: &McpStatusRow, theme: &Theme) -> ListItem<'static> {
    let status = severity_span(row.status.as_str(), row.severity(), theme);
    let path = row
        .config_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    ListItem::new(Line::from(vec![
        Span::styled(format!("{:<18}", row.agent), theme.base_style()),
        Span::styled(" status:", theme.muted_style()),
        status,
        Span::styled(
            format!(" scope:{:<11}", row.scope.as_str()),
            theme.muted_style(),
        ),
        Span::styled(format!(" source:{:<18}", row.source), theme.muted_style()),
        Span::styled(format!(" {path}"), theme.muted_style()),
    ]))
}
