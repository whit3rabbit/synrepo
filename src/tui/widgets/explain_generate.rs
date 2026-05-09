use ratatui::text::{Line, Span};

use crate::tui::app::GenerateCommentaryState;
use crate::tui::theme::Theme;

pub(crate) fn render_generate_commentary(
    state: &GenerateCommentaryState,
    theme: &Theme,
) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Generate commentary...".to_string(),
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "  Up/Down selects scope. Enter queues the run. Esc cancels.".to_string(),
            theme.muted_style(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Scope: ".to_string(), theme.muted_style()),
            Span::styled(state.scope.as_str().to_string(), theme.agent_style()),
        ]),
        Line::from(vec![
            Span::styled("  Input: ".to_string(), theme.muted_style()),
            Span::styled(input_display(&state.input), theme.base_style()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Cost/privacy: generation may call the configured explain provider".to_string(),
            theme.stale_style(),
        )),
        Line::from(Span::styled(
            "  with repository context for the selected scope. Overlay writes happen".to_string(),
            theme.stale_style(),
        )),
        Line::from(Span::styled(
            "  only after you press Enter.".to_string(),
            theme.stale_style(),
        )),
    ]
}

fn input_display(input: &str) -> String {
    if input.is_empty() {
        "<path, symbol, or node id>".to_string()
    } else {
        format!("{input}_")
    }
}
