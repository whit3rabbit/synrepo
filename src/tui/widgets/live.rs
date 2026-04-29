//! Live tab: merged stream of watcher log entries + recent snapshot
//! activity, rendered full-width with a scrollbar and a flash highlight on
//! newly-arrived entries.
//!
//! Source merge: the watcher log (oldest→newest in an `EventLog`) and the
//! snapshot's `recent_activity` (newest-first in `ActivityVm`) are unified
//! into a single chronologically-ordered `Vec<FeedEntry>` at render time.
//!
//! Scroll semantics: `scroll_offset` is rows-up-from-bottom. `0` pins the
//! view to the newest entries; larger values scroll the frame up. Clamping
//! happens here so that a shrinking feed (should never occur because
//! `EventLog` is append-only, but kept defensive) cannot leave the offset
//! past the start of the buffer.

use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    StatefulWidget, Widget,
};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::tui::probe::{ActivityVm, Severity};
use crate::tui::theme::Theme;
use crate::tui::widgets::{severity_span, LogEntry};

/// How fresh an entry must be (seconds) to render in the "flash" style.
const FLASH_WINDOW_SECS: i64 = 3;

/// One merged feed row, borrowing strings from the source `EventLog` /
/// `ActivityVm` to avoid per-frame clones in the render path.
#[derive(Clone, Debug)]
struct FeedEntry<'a> {
    timestamp: &'a str,
    tag: &'a str,
    message: &'a str,
    severity: Severity,
}

/// Render target for the Live tab.
pub struct LiveFeedWidget<'a> {
    /// Watcher event log entries, oldest→newest.
    pub log: &'a [LogEntry],
    /// Recent-activity VM (newest-first from snapshot).
    pub activity: &'a ActivityVm,
    /// Rows-up-from-bottom. Clamped internally against the merged length.
    pub scroll_offset: usize,
    /// True when the view is pinned to the bottom even as new entries arrive.
    pub follow_mode: bool,
    /// Active theme.
    pub theme: &'a Theme,
}

impl<'a> LiveFeedWidget<'a> {
    /// Merge log + activity into a single chronologically-ordered
    /// oldest→newest vector. Both inputs are already sorted (log is
    /// append-only oldest→newest; activity is newest-first), so we reverse
    /// the activity stream and walk both with a two-pointer merge instead of
    /// concat-then-sort.
    fn merged(&self) -> Vec<FeedEntry<'a>> {
        let total = self.log.len() + self.activity.entries.len();
        let mut out: Vec<FeedEntry<'a>> = Vec::with_capacity(total);
        let mut log_iter = self.log.iter().peekable();
        let mut act_iter = self.activity.entries.iter().rev().peekable();
        loop {
            match (log_iter.peek(), act_iter.peek()) {
                (Some(l), Some(a)) => {
                    if l.timestamp.as_str() <= a.timestamp.as_str() {
                        let l = log_iter.next().unwrap();
                        out.push(FeedEntry {
                            timestamp: &l.timestamp,
                            tag: &l.tag,
                            message: &l.message,
                            severity: l.severity,
                        });
                    } else {
                        let a = act_iter.next().unwrap();
                        out.push(FeedEntry {
                            timestamp: &a.timestamp,
                            tag: &a.kind,
                            message: &a.payload,
                            severity: Severity::Healthy,
                        });
                    }
                }
                (Some(_), None) => {
                    let l = log_iter.next().unwrap();
                    out.push(FeedEntry {
                        timestamp: &l.timestamp,
                        tag: &l.tag,
                        message: &l.message,
                        severity: l.severity,
                    });
                }
                (None, Some(_)) => {
                    let a = act_iter.next().unwrap();
                    out.push(FeedEntry {
                        timestamp: &a.timestamp,
                        tag: &a.kind,
                        message: &a.payload,
                        severity: Severity::Healthy,
                    });
                }
                (None, None) => break,
            }
        }
        out
    }
}

impl Widget for LiveFeedWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Span::styled(" live ", self.theme.agent_style()))
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let inner = block.inner(area);
        block.render(area, buf);

        let entries = self.merged();
        if entries.is_empty() {
            Paragraph::new("(no events yet — watcher idle)")
                .style(self.theme.muted_style())
                .render(inner, buf);
            return;
        }

        let visible = inner.height as usize;
        let total = entries.len();
        let max_scroll = total.saturating_sub(visible.max(1));
        let effective_offset = if self.follow_mode {
            0
        } else {
            self.scroll_offset.min(max_scroll)
        };
        let end = total.saturating_sub(effective_offset);
        let start = end.saturating_sub(visible.max(1));

        let now = OffsetDateTime::now_utc();
        let items: Vec<ListItem> = entries[start..end]
            .iter()
            .map(|e| feed_row(e, self.theme, now))
            .collect();

        // Reserve one column on the right for the scrollbar.
        let list_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: inner.height,
        };
        Widget::render(
            List::new(items).style(self.theme.base_style()),
            list_area,
            buf,
        );

        if total > visible {
            let mut scroll_state =
                ScrollbarState::new(max_scroll).position(max_scroll - effective_offset);
            let scrollbar_area = inner.inner(Margin {
                vertical: 0,
                horizontal: 0,
            });
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(self.theme.border_style())
                .render(scrollbar_area, buf, &mut scroll_state);
        }
    }
}

/// Render a single feed row with timestamp, tag, message, and an optional
/// "flash" highlight when the entry arrived within the last
/// [`FLASH_WINDOW_SECS`] seconds.
fn feed_row(entry: &FeedEntry<'_>, theme: &Theme, now: OffsetDateTime) -> ListItem<'static> {
    let ts_display = if entry.timestamp.is_empty() {
        "      -      ".to_string()
    } else {
        entry.timestamp.to_string()
    };
    let is_fresh = entry_is_fresh(entry.timestamp, now);
    let tag_base = theme.watch_active_style();
    let tag_style = if is_fresh {
        tag_base.add_modifier(Modifier::BOLD)
    } else {
        tag_base
    };
    let message_span = severity_span(entry.message, entry.severity, theme);
    let message_span = if is_fresh {
        let style = message_span.style.add_modifier(Modifier::BOLD);
        Span::styled(message_span.content.into_owned(), style)
    } else {
        message_span
    };
    let ts_style: Style = if is_fresh {
        theme.agent_style().add_modifier(Modifier::BOLD)
    } else {
        theme.muted_style()
    };
    let mut spans = Vec::new();
    if is_fresh && (theme.accessibility.no_color || theme.accessibility.ascii_only) {
        spans.push(Span::styled("new ", theme.agent_style()));
    }
    spans.extend([
        Span::styled(ts_display, ts_style),
        Span::raw(" "),
        Span::styled(format!("[{}]", entry.tag), tag_style),
        Span::raw(" "),
        message_span,
    ]);
    ListItem::new(Line::from(spans))
}

/// Parse an RFC 3339 timestamp and return true when it is within
/// [`FLASH_WINDOW_SECS`] of `now`. Unparseable or empty stamps return false —
/// the flash is a bonus, not a correctness signal.
fn entry_is_fresh(stamp: &str, now: OffsetDateTime) -> bool {
    if stamp.is_empty() {
        return false;
    }
    match OffsetDateTime::parse(stamp, &Rfc3339) {
        Ok(t) => {
            let delta = (now - t).whole_seconds();
            (0..=FLASH_WINDOW_SECS).contains(&delta)
        }
        Err(_) => false,
    }
}
