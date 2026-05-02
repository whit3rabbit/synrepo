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
    /// Registry-managed repos and switching.
    Repos,
    /// Merged watcher + recent-activity feed.
    Live,
    /// System-health pane.
    Health,
    /// Trust signals for context quality and advisory overlay freshness.
    Trust,
    /// Explain status, totals, and refresh actions.
    Explain,
    /// Next-actions + quick-actions.
    Actions,
    /// Active-project MCP registration status.
    Mcp,
}

impl fmt::Display for ActiveTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveTab::Repos => write!(f, "Repos"),
            ActiveTab::Live => write!(f, "Live"),
            ActiveTab::Health => write!(f, "Health"),
            ActiveTab::Trust => write!(f, "Trust"),
            ActiveTab::Explain => write!(f, "Explain"),
            ActiveTab::Actions => write!(f, "Actions"),
            ActiveTab::Mcp => write!(f, "MCP"),
        }
    }
}

impl AppState {
    /// Switch to a specific tab. Resets scroll when leaving Live so a return
    /// visit starts pinned to the bottom. Also clears the explain folder
    /// picker so it never survives a tab switch.
    pub fn set_tab(&mut self, tab: ActiveTab) {
        if self.active_tab != tab {
            self.active_tab = tab;
            self.picker = None;
            if matches!(tab, ActiveTab::Explain) {
                self.refresh_explain_preview(false);
            } else if matches!(tab, ActiveTab::Repos) {
                self.ensure_explore_projects_fresh();
            } else if matches!(tab, ActiveTab::Live) {
                // Re-enter Live pinned to the tail so operators always see
                // the most recent entries on tab switch.
                self.scroll_offset = 0;
                self.follow_mode = true;
            }
        }
    }

    /// Advance to the next tab in Repos → Live → Health → Trust → Explain → Actions → MCP order.
    pub fn cycle_tab(&mut self) {
        let next = match self.active_tab {
            ActiveTab::Repos => ActiveTab::Live,
            ActiveTab::Live => ActiveTab::Health,
            ActiveTab::Health => ActiveTab::Trust,
            ActiveTab::Trust => ActiveTab::Explain,
            ActiveTab::Explain => ActiveTab::Actions,
            ActiveTab::Actions => ActiveTab::Mcp,
            ActiveTab::Mcp => ActiveTab::Repos,
        };
        self.set_tab(next);
    }

    /// Reverse of `cycle_tab`. Used by Shift-Tab and Left arrow.
    pub fn cycle_tab_back(&mut self) {
        let prev = match self.active_tab {
            ActiveTab::Repos => ActiveTab::Mcp,
            ActiveTab::Live => ActiveTab::Repos,
            ActiveTab::Health => ActiveTab::Live,
            ActiveTab::Trust => ActiveTab::Health,
            ActiveTab::Explain => ActiveTab::Trust,
            ActiveTab::Actions => ActiveTab::Explain,
            ActiveTab::Mcp => ActiveTab::Actions,
        };
        self.set_tab(prev);
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
        self.scroll_up(self.page_rows());
    }

    /// Scroll down by one page. Re-enables follow when the frame reaches the
    /// bottom.
    pub fn page_down(&mut self) {
        self.scroll_down(self.page_rows());
    }

    fn page_rows(&self) -> usize {
        self.live_visible_rows.saturating_sub(2).max(1)
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
