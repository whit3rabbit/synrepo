//! Tab-switch and Live-tab scroll helpers for [`AppState`]. Split out of
//! `app/mod.rs` so the core state machine stays under the 400-line cap.
//!
//! Scroll semantics match [`crate::tui::widgets::live::LiveFeedWidget`]:
//! `scroll_offset` is rows-up-from-bottom. `0` pins the view to the newest
//! entry; incrementing scrolls the frame up.

use std::fmt;

use super::AppState;

/// Which dashboard tab is currently rendered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveTab {
    /// Merged watcher + recent-activity feed.
    Live,
    /// System-health pane.
    Health,
    /// Next-actions + quick-actions.
    Actions,
}

impl fmt::Display for ActiveTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveTab::Live => write!(f, "Live"),
            ActiveTab::Health => write!(f, "Health"),
            ActiveTab::Actions => write!(f, "Actions"),
        }
    }
}

/// Approximate number of feed rows one PgUp/PgDn should traverse. The Live
/// widget subtracts two rows for headroom, so this matches that intent
/// without plumbing the actual visible-rows count down into the state.
const PAGE_ROWS: usize = 18;

impl AppState {
    /// Switch to a specific tab. Resets scroll when leaving Live so a return
    /// visit starts pinned to the bottom.
    pub fn set_tab(&mut self, tab: ActiveTab) {
        if self.active_tab != tab {
            self.active_tab = tab;
            if matches!(tab, ActiveTab::Live) {
                // Re-enter Live pinned to the tail so operators always see
                // the most recent entries on tab switch.
                self.scroll_offset = 0;
                self.follow_mode = true;
            }
        }
    }

    /// Advance to the next tab in Live → Health → Actions → Live order.
    pub fn cycle_tab(&mut self) {
        let next = match self.active_tab {
            ActiveTab::Live => ActiveTab::Health,
            ActiveTab::Health => ActiveTab::Actions,
            ActiveTab::Actions => ActiveTab::Live,
        };
        self.set_tab(next);
    }

    /// Scroll the Live feed up by `rows` entries. Disables follow so new
    /// entries no longer snap the frame back to the bottom.
    pub fn scroll_up(&mut self, rows: usize) {
        self.follow_mode = false;
        self.scroll_offset = self.scroll_offset.saturating_add(rows);
    }

    /// Scroll the Live feed down by `rows` entries. Re-enables follow when
    /// the frame reaches the bottom so new entries resume snapping.
    pub fn scroll_down(&mut self, rows: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(rows);
        if self.scroll_offset == 0 {
            self.follow_mode = true;
        }
    }

    /// Scroll up by one "page" (approx. visible rows minus 2).
    pub fn page_up(&mut self) {
        self.scroll_up(PAGE_ROWS);
    }

    /// Scroll down by one page. Re-enables follow when the frame reaches the
    /// bottom.
    pub fn page_down(&mut self) {
        self.scroll_down(PAGE_ROWS);
    }

    /// Pin the Live feed to the oldest entry. Disables follow mode.
    pub fn scroll_home(&mut self) {
        self.follow_mode = false;
        self.scroll_offset = usize::MAX / 2;
    }

    /// Pin the Live feed to the newest entry and re-enable follow.
    pub fn scroll_end(&mut self) {
        self.scroll_offset = 0;
        self.follow_mode = true;
    }

    /// Flip the follow-mode flag. When enabling, also snap to the bottom so
    /// the next render reflects the intent.
    pub fn toggle_follow(&mut self) {
        self.follow_mode = !self.follow_mode;
        if self.follow_mode {
            self.scroll_offset = 0;
        }
    }
}
