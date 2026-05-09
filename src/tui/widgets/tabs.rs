//! Dashboard tab bar. Wraps `ratatui::widgets::Tabs` to keep styling in one
//! place and render the operator tabs with a keyboard-hint prefix so the active tab is visible without
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
            ("1", "Repos", ActiveTab::Repos),
            ("2", "Live", ActiveTab::Live),
            ("3", "Health", ActiveTab::Health),
            ("4", "Actions", ActiveTab::Actions),
            ("5", "Explain", ActiveTab::Explain),
            ("6", "Integrations", ActiveTab::Mcp),
            ("7", "Suggestion", ActiveTab::Suggestion),
            ("8", "Trust", ActiveTab::Trust),
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
                ActiveTab::Repos => 0,
                ActiveTab::Live => 1,
                ActiveTab::Health => 2,
                ActiveTab::Actions => 3,
                ActiveTab::Explain => 4,
                ActiveTab::Mcp => 5,
                ActiveTab::Suggestion => 6,
                ActiveTab::Trust => 7,
            })
            .style(self.theme.base_style())
            .highlight_style(self.theme.selected_style())
            .divider(Span::styled("  ", self.theme.muted_style()))
            .block(block)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;

    use super::*;

    #[test]
    fn tab_bar_labels_agent_status_as_integrations() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);
        DashboardTabsWidget {
            active: ActiveTab::Mcp,
            theme: &Theme::plain(),
        }
        .render(area, &mut buf);
        let text = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("[4] Actions"));
        assert!(text.contains("[6] Integrations"));
        assert!(text.contains("[8] Trust"));
    }
}
