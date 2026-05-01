//! Widget tree for the dashboard. The tabs-era split keeps each widget in its
//! own file; this module re-exports the public surface, owns the two value
//! types every tab shares (`LogEntry`, `QuickAction`), and holds the shared
//! `severity_span` styling helper.

use ratatui::text::Span;

use crate::tui::probe::Severity;
use crate::tui::theme::Theme;

pub mod actions;
pub mod explain;
pub(crate) mod explore;
pub mod footer;
pub mod header;
pub mod health;
pub mod live;
pub mod mcp;
pub(crate) mod projects;
pub mod tabs;
pub mod trust;

pub use actions::ActionsTabWidget;
pub use explain::ExplainTabWidget;
pub(crate) use explore::ExploreTabWidget;
pub use footer::FooterWidget;
pub use header::HeaderWidget;
pub use health::HealthWidget;
pub use live::LiveFeedWidget;
pub use mcp::McpTabWidget;
pub(crate) use projects::ProjectPickerWidget;
pub use tabs::DashboardTabsWidget;
pub use trust::TrustWidget;

/// One ring-buffer entry for the event/notification log pane. Shared between
/// `AppState::log` and the merged-feed widget in `widgets/live.rs`.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// RFC-3339 timestamp when the entry was pushed. Empty for unknown.
    pub timestamp: String,
    /// Tag such as "watch", "reconcile", "lock".
    pub tag: String,
    /// Free-form line.
    pub message: String,
    /// Severity used for color.
    pub severity: Severity,
}

/// One row in the Actions tab's quick-actions list. Key binding + label +
/// optional disabled state.
#[derive(Clone, Debug)]
pub struct QuickAction {
    /// Key binding display label (e.g. "s").
    pub key: String,
    /// Human-readable action label.
    pub label: String,
    /// True when the action is disabled in the current context (e.g. "stop
    /// watch" when nothing is running).
    pub disabled: bool,
    /// True when invoking the action should open a confirmation prompt first.
    pub requires_confirm: bool,
    /// True when the action can remove files or durable state.
    pub destructive: bool,
    /// True when the action can take noticeable time or trigger heavy work.
    pub expensive: bool,
    /// Optional command-palette name for the same action.
    pub command_label: Option<String>,
}

/// Render a text span colored by severity. On plain-theme terminals a glyph
/// prefix is embedded so the distinction survives loss of color.
pub(crate) fn severity_span(text: &str, sev: Severity, theme: &Theme) -> Span<'static> {
    let style = match sev {
        Severity::Healthy => theme.healthy_style(),
        Severity::Stale => theme.stale_style(),
        Severity::Blocked => theme.blocked_style(),
    };
    let prefix = match theme.variant {
        crate::tui::theme::ThemeVariant::Plain => match sev {
            Severity::Healthy => "[ok] ",
            Severity::Stale => "[warn] ",
            Severity::Blocked => "[blocked] ",
        },
        crate::tui::theme::ThemeVariant::Dark => "",
    };
    Span::styled(format!("{prefix}{text}"), style)
}
