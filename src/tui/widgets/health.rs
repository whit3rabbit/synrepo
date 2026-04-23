//! System-health pane: graph counts, export freshness, commentary coverage,
//! overlay cost, explain state. Each row is a label/value pair with a
//! severity color on the value.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::tui::probe::HealthVm;
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// System-health pane widget.
pub struct HealthWidget<'a> {
    /// Flattened rows.
    pub vm: &'a HealthVm,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for HealthWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" system health ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let items: Vec<ListItem> = self
            .vm
            .rows
            .iter()
            .map(|row| {
                let label = Span::styled(
                    format!("{:<14}", format!("{}:", row.label)),
                    self.theme.muted_style(),
                );
                let value = severity_span(&row.value, row.severity, self.theme);
                ListItem::new(Line::from(vec![label, value]))
            })
            .collect();
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}
