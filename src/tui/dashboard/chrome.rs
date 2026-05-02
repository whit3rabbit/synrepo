//! Shared dashboard chrome helpers.

use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::theme::Theme;

pub(super) fn draw_help(frame: &mut ratatui::Frame, theme: Theme) {
    let block = Block::default()
        .title(" help ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    let lines = vec![
        Line::from("[p] projects    [?] help    [:] commands    [q] quit"),
        Line::from("[Tab/Shift-Tab/←/→/1-7] tabs  [r] refresh  [w] watch for active project"),
        Line::from("Project picker: filter, Enter switch, r rename, a add cwd, d detach, w watch"),
    ];
    let paragraph = Paragraph::new(lines).block(block).style(theme.base_style());
    frame.render_widget(paragraph, frame.area());
}

pub(super) fn draw_command_palette(frame: &mut ratatui::Frame, theme: Theme) {
    let block = Block::default()
        .title(" commands ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    let lines = vec![
        Line::from("project switch"),
        Line::from("project add current directory"),
        Line::from("project detach selected"),
        Line::from("watch start/stop selected project"),
    ];
    let paragraph = Paragraph::new(lines).block(block).style(theme.base_style());
    frame.render_widget(paragraph, frame.area());
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
