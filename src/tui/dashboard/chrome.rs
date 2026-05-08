//! Shared dashboard chrome helpers.

use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::tui::projects::GlobalAppState;
use crate::tui::theme::Theme;

pub(super) fn draw_help(frame: &mut ratatui::Frame, theme: Theme) {
    let block = Block::default()
        .title(" help ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    let lines = vec![
        Line::from("[p] projects    [?] help    [:] commands    [q] quit"),
        Line::from("[Tab/Shift-Tab/Left/Right/1-8] tabs  [Esc] Live/cancel  [r] refresh"),
        Line::from("Explain: r refresh, a all stale, c changed, f folders, d/D/x/X docs"),
        Line::from(
            "Project picker: filter, Enter switch, r rename, a add cwd, d detach confirm, w watch",
        ),
    ];
    let paragraph = Paragraph::new(lines).block(block).style(theme.base_style());
    frame.render_widget(paragraph, frame.area());
}

pub(super) fn draw_command_palette(frame: &mut ratatui::Frame, state: &GlobalAppState) {
    let theme = state.theme;
    let filter = state
        .command_palette
        .as_ref()
        .map(|palette| palette.filter.as_str())
        .unwrap_or("");
    let selected = state
        .command_palette
        .as_ref()
        .map(|palette| palette.selected)
        .unwrap_or(0);
    let title = if filter.is_empty() {
        " commands ".to_string()
    } else {
        format!(" commands /{filter} ")
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    let items = state.filtered_command_palette_items();
    if items.is_empty() {
        let text = if filter.is_empty() {
            "  no commands available".to_string()
        } else {
            format!("  no commands match /{filter}")
        };
        let paragraph = Paragraph::new(Line::from(text))
            .block(block)
            .style(theme.muted_style());
        frame.render_widget(paragraph, frame.area());
        return;
    }
    let rows = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let marker = if idx == selected { ">" } else { " " };
            let style = if idx == selected {
                theme.selected_style()
            } else if item.disabled_reason.is_some() {
                theme.muted_style()
            } else {
                theme.base_style()
            };
            let mut spans = vec![
                Span::styled(format!("{marker} {}", item.prefix()), style),
                Span::styled(format!(" {}", item.label), style),
            ];
            if let Some(reason) = &item.disabled_reason {
                spans.push(Span::styled(format!(" ({reason})"), theme.muted_style()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(rows).block(block), frame.area());
}

/// Minimum terminal size where every tab still renders something usable.
/// Header (5) + tabs (3) + footer (1) = 9 rows of chrome; the Trust tab is
/// the densest at three stacked panes, so the floor is set to leave at least
/// 11 rows of content area (20 total). Width 60 keeps footer-hint compaction
/// working without compacting to a single `[q]`.
const MIN_TERMINAL_WIDTH: u16 = 60;
const MIN_TERMINAL_HEIGHT: u16 = 20;

/// Render a centered "terminal too small" warning when the frame is below
/// the documented minimum. Returns true when the warning was rendered, in
/// which case the caller should skip the rest of the dashboard render.
pub(super) fn draw_too_small_warning(frame: &mut ratatui::Frame, theme: &Theme) -> bool {
    let size = frame.area();
    if size.width >= MIN_TERMINAL_WIDTH && size.height >= MIN_TERMINAL_HEIGHT {
        return false;
    }
    let lines = vec![
        Line::from(format!(
            "terminal too small: {}x{}",
            size.width, size.height
        )),
        Line::from(format!(
            "synrepo dashboard needs at least {MIN_TERMINAL_WIDTH}x{MIN_TERMINAL_HEIGHT}."
        )),
        Line::from("resize and the view will redraw automatically."),
    ];
    let paragraph = Paragraph::new(lines).style(theme.stale_style());
    frame.render_widget(paragraph, size);
    true
}
