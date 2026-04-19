//! Header widget: repo path, reconcile/watch/lock/MCP status row, and a
//! hand-rolled braille spinner that animates while a reconcile pass is in
//! flight. The spinner frame index lives on `AppState::frame`; the widget
//! stays stateless and picks a glyph from [`SPINNER_FRAMES`] modulo the
//! array length.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::tui::probe::HeaderVm;
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// Braille spinner frames. Kept in sync with
/// `throbber_widgets_tui::BRAILLE_SIX` so a future re-introduction of the
/// crate would be drop-in.
const SPINNER_FRAMES: [&str; 10] = [
    "\u{2807}", "\u{2811}", "\u{2819}", "\u{2838}", "\u{2830}", "\u{2834}", "\u{2826}", "\u{2807}",
    "\u{2811}", "\u{2819}",
];

/// Pick the spinner glyph for a given monotonic frame counter.
pub fn spinner_glyph(frame: u32) -> &'static str {
    SPINNER_FRAMES[(frame as usize) % SPINNER_FRAMES.len()]
}

/// Header widget showing repo path, mode, reconcile/watch/lock/MCP states,
/// plus a spinner when `reconcile_active` is true.
pub struct HeaderWidget<'a> {
    /// Header view model built from the status snapshot + probe report.
    pub vm: &'a HeaderVm,
    /// Active theme.
    pub theme: &'a Theme,
    /// Monotonic tick counter; advanced each tick by the app shell. Drives
    /// the spinner animation.
    pub frame: u32,
    /// True when a reconcile pass is in flight (spinner visible). False shows
    /// an idle marker instead.
    pub reconcile_active: bool,
}

impl Widget for HeaderWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Span::styled(" synrepo ", self.theme.agent_style()))
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());

        // Left-most column of the repo-path row is a spinner or idle marker so
        // the operator sees liveness feedback regardless of which tab is up.
        let status_span: Span<'static> = if self.reconcile_active {
            Span::styled(
                format!("{} reconciling  ", spinner_glyph(self.frame)),
                self.theme.watch_active_style(),
            )
        } else {
            Span::styled("[idle]  ", self.theme.muted_style())
        };

        let mode_span = Span::styled(
            format!(" mode: {}", self.vm.mode_label),
            self.theme.base_style(),
        );
        let reconcile_span = severity_span(
            &format!("reconcile: {}", self.vm.reconcile_label),
            self.vm.reconcile_severity,
            self.theme,
        );
        let watch_span = severity_span(
            &format!("watch: {}", self.vm.watch_label),
            self.vm.watch_severity,
            self.theme,
        );
        let lock_span = severity_span(
            &format!("lock: {}", self.vm.lock_label),
            self.vm.lock_severity,
            self.theme,
        );
        let mcp_span = severity_span(
            &format!("mcp: {}", self.vm.mcp_label),
            self.vm.mcp_severity,
            self.theme,
        );

        let lines = vec![
            Line::from(vec![
                status_span,
                Span::styled(self.vm.repo_display.clone(), self.theme.muted_style()),
            ]),
            Line::from(vec![
                mode_span,
                Span::raw("  "),
                reconcile_span,
                Span::raw("  "),
                watch_span,
                Span::raw("  "),
                lock_span,
                Span::raw("  "),
                mcp_span,
            ]),
        ];
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_glyph_wraps_on_frame_overflow() {
        let first = spinner_glyph(0);
        let same = spinner_glyph(SPINNER_FRAMES.len() as u32);
        assert_eq!(
            first,
            same,
            "spinner must wrap back to frame 0 after {} ticks",
            SPINNER_FRAMES.len()
        );
    }

    #[test]
    fn spinner_glyph_advances_between_adjacent_frames() {
        let a = spinner_glyph(0);
        let b = spinner_glyph(1);
        assert_ne!(a, b, "first two frames should differ");
    }
}
