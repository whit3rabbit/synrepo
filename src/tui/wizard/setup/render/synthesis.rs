//! Rendering for the synthesis sub-flow of the setup wizard.
//!
//! Kept separate from `render/mod.rs` so the parent file stays under the
//! 400-line limit and so synthesis-specific UX tweaks live in one place.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::super::state::SetupWizardState;
use super::super::synthesis::{CloudProvider, LOCAL_PRESETS, SYNTHESIS_ROWS};
use crate::tui::theme::Theme;

/// Draw the cloud-vs-local-vs-skip selection list.
pub(super) fn draw_synthesis_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let items: Vec<ListItem> = SYNTHESIS_ROWS
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = i == state.synthesis_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme.agent_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{marker}{}", row.label()),
                style,
            )))
        })
        .collect();

    let block = Block::default()
        .title(" LLM synthesis (optional) ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}

/// Draw the local-LLM preset list (Ollama / llama.cpp / LM Studio / vLLM /
/// Custom).
pub(super) fn draw_local_preset_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let items: Vec<ListItem> = LOCAL_PRESETS
        .iter()
        .enumerate()
        .map(|(i, preset)| {
            let selected = i == state.local_preset_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme.agent_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{marker}{}", preset.label()),
                style,
            )))
        })
        .collect();

    let block = Block::default()
        .title(" local LLM preset ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}

/// Draw the endpoint-URL text-input step, pre-filled with the preset default.
pub(super) fn draw_local_endpoint_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let buffer = state.endpoint_input.value();
    let cursor = state.endpoint_input.cursor();
    // Render with a simple caret marker. Unicode-safe because cursor is a
    // char index; split via `.chars()` rather than byte slicing.
    let prefix: String = buffer.chars().take(cursor).collect();
    let suffix: String = buffer.chars().skip(cursor).collect();
    let input_line = Line::from(vec![
        Span::styled(prefix, theme.base_style()),
        Span::styled("│", theme.agent_style()),
        Span::styled(suffix, theme.base_style()),
    ]);

    let lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!("Preset: {}", state.local_preset.label()),
            theme.muted_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Endpoint URL (edit to match your local server):",
            theme.base_style(),
        )),
        input_line,
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "/v1/chat/completions → OpenAI-compatible (llama.cpp, LM Studio, vLLM)",
            theme.muted_style(),
        )),
        Line::from(Span::styled(
            "any other path         → Ollama native (/api/chat)",
            theme.muted_style(),
        )),
    ];

    let block = Block::default()
        .title(" local endpoint ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Map a [`CloudProvider`] to the env-var name the user must export.
pub(super) fn provider_env_var(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::Anthropic => "ANTHROPIC_API_KEY",
        CloudProvider::OpenAi => "OPENAI_API_KEY",
        CloudProvider::Gemini => "GEMINI_API_KEY",
        CloudProvider::OpenRouter => "OPENROUTER_API_KEY",
    }
}
