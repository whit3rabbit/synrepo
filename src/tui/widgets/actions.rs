//! Actions tab: next-actions derived from health signals on top, explicit
//! quick-actions (key-binding + label + disabled flag) below. Both panes
//! used to share the right column with health/activity; now each gets its
//! own full-width section on its own tab.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::tui::app::ConfirmStopWatchState;
use crate::tui::probe::NextAction;
use crate::tui::theme::Theme;
use crate::tui::widgets::confirm_stop_watch::render_confirm_stop_watch;
use crate::tui::widgets::{severity_span, QuickAction};

/// Actions tab widget: next-actions stacked above quick-actions.
pub struct ActionsTabWidget<'a> {
    /// Next actions derived from health signals.
    pub next_actions: &'a [NextAction],
    /// Explicit key-bound actions.
    pub quick_actions: &'a [QuickAction],
    /// Active confirm-stop-watch modal state, when an action is gated.
    pub confirm_stop_watch: Option<&'a ConfirmStopWatchState>,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for ActionsTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if let Some(confirm) = self.confirm_stop_watch {
            let block = Block::default()
                .title(" actions ")
                .borders(Borders::ALL)
                .border_style(self.theme.border_style());
            let items: Vec<ListItem> = render_confirm_stop_watch(confirm, self.theme)
                .into_iter()
                .map(ListItem::new)
                .collect();
            List::new(items)
                .block(block)
                .style(self.theme.base_style())
                .render(area, buf);
            return;
        }

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

        // Bottom: runnable commands.
        let quick_block = Block::default()
            .title(" run commands ")
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
                    Span::styled(
                        format!(" {} [{}] ", quick_action_prefix(a), a.key),
                        key_style,
                    ),
                    Span::styled(a.label.clone(), label_style),
                ]))
            })
            .collect();
        List::new(quick_items)
            .block(quick_block)
            .render(rows[1], buf);
    }
}

fn quick_action_prefix(action: &QuickAction) -> &'static str {
    if action.disabled {
        "x"
    } else if action.destructive {
        "!"
    } else if action.expensive {
        "~"
    } else if action.requires_confirm {
        "?"
    } else {
        " "
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action() -> QuickAction {
        QuickAction {
            key: "x".to_string(),
            label: "demo".to_string(),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: None,
        }
    }

    #[test]
    fn quick_action_prefix_marks_plain_mode_state() {
        let mut a = action();
        assert_eq!(quick_action_prefix(&a), " ");
        a.expensive = true;
        assert_eq!(quick_action_prefix(&a), "~");
        a.destructive = true;
        assert_eq!(quick_action_prefix(&a), "!");
        a.disabled = true;
        assert_eq!(quick_action_prefix(&a), "x");
    }

    #[test]
    fn actions_tab_labels_quick_actions_as_runnable_commands() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let actions = vec![action()];
        ActionsTabWidget {
            next_actions: &[],
            quick_actions: &actions,
            confirm_stop_watch: None,
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
        assert!(text.contains("run commands"));
    }
}
