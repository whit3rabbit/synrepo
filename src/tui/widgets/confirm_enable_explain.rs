//! Shared enable-explain confirmation body.

use ratatui::text::{Line, Span};

use crate::pipeline::explain::ExplainStatus;
use crate::surface::status_snapshot::StatusSnapshot;
use crate::tui::app::{describe_pending_mode, ConfirmEnableExplainState};
use crate::tui::theme::Theme;

pub(crate) fn render_confirm_enable_explain(
    confirm: &ConfirmEnableExplainState,
    snapshot: &StatusSnapshot,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let scope = describe_pending_mode(&confirm.mode);
    let mut lines = vec![
        Line::from(Span::styled(
            "Optional explain is off.".to_string(),
            theme.stale_style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Configure optional explain before running this scope?".to_string(),
            theme.muted_style(),
        )),
    ];
    if let Some(env_var) = detected_env_var(snapshot) {
        lines.push(Line::from(Span::styled(
            format!("  Detected ${env_var}; setup can opt in without adding a key first."),
            theme.agent_style(),
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled("  Scope: ".to_string(), theme.muted_style()),
            Span::styled(scope, theme.base_style()),
        ]),
        Line::from(""),
        action_line("y", "Configure optional explain", theme),
        action_line("n", "Cancel", theme),
    ]);
    lines
}

fn detected_env_var(snapshot: &StatusSnapshot) -> Option<&'static str> {
    match snapshot
        .explain_provider
        .as_ref()
        .map(|display| &display.status)
    {
        Some(ExplainStatus::DisabledKeyDetected { env_var }) => Some(env_var),
        _ => None,
    }
}

fn action_line(key: &str, label: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  [{key}] "), theme.agent_style()),
        Span::styled(label.to_string(), theme.base_style()),
    ])
}
