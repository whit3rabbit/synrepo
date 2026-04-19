//! Actions tab: next-actions derived from health signals on top, explicit
//! quick-actions (key-binding + label + disabled flag) below. Both panes
//! used to share the right column with health/activity; now each gets its
//! own full-width section on its own tab.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::tui::probe::NextAction;
use crate::tui::theme::Theme;
use crate::tui::widgets::{severity_span, QuickAction};

/// Actions tab widget: next-actions stacked above quick-actions.
pub struct ActionsTabWidget<'a> {
    /// Next actions derived from health signals.
    pub next_actions: &'a [NextAction],
    /// Explicit key-bound actions.
    pub quick_actions: &'a [QuickAction],
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for ActionsTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Top: next actions.
        let next_block = Block::default()
            .title(" next actions ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let next_items: Vec<ListItem> = self
            .next_actions
            .iter()
            .map(|a| ListItem::new(Line::from(severity_span(&a.label, a.severity, self.theme))))
            .collect();
        List::new(next_items).block(next_block).render(rows[0], buf);

        // Bottom: quick actions.
        let quick_block = Block::default()
            .title(" quick actions ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let quick_items: Vec<ListItem> = self
            .quick_actions
            .iter()
            .map(|a| {
                let (label_style, key_style) = if a.disabled {
                    (self.theme.muted_style(), self.theme.muted_style())
                } else {
                    (self.theme.base_style(), self.theme.agent_style())
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" [{}] ", a.key), key_style),
                    Span::styled(a.label.clone(), label_style),
                ]))
            })
            .collect();
        List::new(quick_items)
            .block(quick_block)
            .render(rows[1], buf);
    }
}
