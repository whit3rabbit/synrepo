//! Shared stop-watch confirmation body.

use ratatui::text::{Line, Span};

use crate::tui::app::{
    describe_pending_stop_action, ConfirmStopWatchState, PendingStopWatchAction,
};
use crate::tui::theme::Theme;

pub(crate) fn render_confirm_stop_watch(
    confirm: &ConfirmStopWatchState,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let scope = describe_pending_stop_action(&confirm.pending);
    let (need, action) = match &confirm.pending {
        PendingStopWatchAction::Explain(_) => (
            "Explain needs the writer lock, which watch currently holds.",
            "Stop watch and run explain",
        ),
    };
    vec![
        Line::from(Span::styled(
            "Watch service is active.".to_string(),
            theme.stale_style(),
        )),
        Line::from(""),
        Line::from(Span::styled(format!("  {need}"), theme.muted_style())),
        Line::from(Span::styled(
            "  Stop watch to continue; restart it later with `synrepo watch`.".to_string(),
            theme.muted_style(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Scope: ".to_string(), theme.muted_style()),
            Span::styled(scope, theme.base_style()),
        ]),
        Line::from(""),
        action_line("y", action, theme),
        action_line("n", "Cancel", theme),
    ]
}

fn action_line(key: &str, label: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  [{key}] "), theme.agent_style()),
        Span::styled(label.to_string(), theme.base_style()),
    ])
}
