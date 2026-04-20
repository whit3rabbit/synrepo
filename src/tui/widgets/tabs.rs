//! Dashboard tab bar. Wraps `ratatui::widgets::Tabs` to keep styling in one
//! place and render the four operator tabs (Live / Health / Synthesis /
//! Actions) with a keyboard-hint prefix so the active tab is visible without
//! color.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs, Widget};

use crate::tui::app::ActiveTab;
use crate::tui::theme::Theme;

/// Top-of-content tab bar.
pub struct DashboardTabsWidget<'a> {
    /// Currently-selected tab.
    pub active: ActiveTab,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for DashboardTabsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());

        let titles: Vec<Line<'static>> = [
            ("1", "Live", ActiveTab::Live),
            ("2", "Health", ActiveTab::Health),
            ("3", "Synthesis", ActiveTab::Synthesis),
            ("4", "Actions", ActiveTab::Actions),
        ]
        .into_iter()
        .map(|(key, label, _)| {
            Line::from(vec![
                Span::styled(format!("[{key}] "), self.theme.agent_style()),
                Span::styled(label.to_string(), self.theme.base_style()),
            ])
        })
        .collect();

        Tabs::new(titles)
            .select(match self.active {
                ActiveTab::Live => 0,
                ActiveTab::Health => 1,
                ActiveTab::Synthesis => 2,
                ActiveTab::Actions => 3,
            })
            .style(self.theme.base_style())
            .highlight_style(self.theme.selected_style())
            .divider(Span::styled("  ", self.theme.muted_style()))
            .block(block)
            .render(area, buf);
    }
}
