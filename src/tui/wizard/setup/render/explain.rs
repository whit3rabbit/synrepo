//! Rendering for the explain sub-flow of the setup wizard.
//!
//! Kept separate from `render/mod.rs` so the parent file stays under the
//! 400-line limit and so explain-specific UX tweaks live in one place.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::super::explain::{
    CloudCredentialSource, CloudProvider, ExplainChoice, EXPLAIN_ROWS, LOCAL_PRESETS,
};
use super::super::state::SetupWizardState;
use crate::tui::theme::Theme;

/// Canonical commentary example used in the explainer step. Drawn from a real
/// refresh run on this repo's `writer` module so the operator sees the actual
/// voice and granularity, not a sanitised placeholder.
const COMMENTARY_EXAMPLE: &str =
    "writer.rs acquires a per-repo advisory lock on `.synrepo/state/writer.lock` \
     via fs2 and retries briefly on WouldBlock to mask flock release latency. \
     Holders stamp a JSON sidecar with pid + acquired_at for external diagnostics.";

/// Canonical cross-link candidate example used in the explainer step.
const CROSS_LINK_EXAMPLE: &str =
    "docs/adr/0004-writer-lock.md  ──Governs──▶  src/pipeline/writer/mod.rs \
     (tier: high-conf, stored in overlay; promote via `synrepo links accept`)";

/// Draw the cloud-vs-local-vs-skip selection list.
pub(super) fn draw_explain_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let items: Vec<ListItem> = EXPLAIN_ROWS
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = i == state.explain_cursor;
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
        .title(" LLM explain (optional) ")
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

/// Draw the masked cloud-provider API-key entry step.
pub(super) fn draw_cloud_api_key_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let provider = state
        .pending_cloud_provider
        .unwrap_or(CloudProvider::Anthropic);
    let buffer = state.api_key_input.value();
    let cursor = state.api_key_input.cursor();
    let masked: String = buffer.chars().map(|_| '*').collect();
    let prefix: String = masked.chars().take(cursor).collect();
    let suffix: String = masked.chars().skip(cursor).collect();
    let input_line = Line::from(vec![
        Span::styled(prefix, theme.base_style()),
        Span::styled("│", theme.agent_style()),
        Span::styled(suffix, theme.base_style()),
    ]);

    let lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!("Provider: {}", provider.label()),
            theme.muted_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            format!(
                "No {} was detected in the current shell or ~/.synrepo/config.toml.",
                provider.env_var()
            ),
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "Enter an API key now to save it in ~/.synrepo/config.toml on apply:",
            theme.base_style(),
        )),
        input_line,
    ];

    let block = Block::default()
        .title(" cloud API key ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Map a [`CloudProvider`] to the env-var name the user must export.
pub(super) fn provider_env_var(provider: CloudProvider) -> &'static str {
    provider.env_var()
}

/// Draw the static "what explain does" explainer. Renders a real commentary
/// example and a real cross-link candidate so the operator can see concretely
/// what they are opting into before they pick a provider.
pub(super) fn draw_explain_explain_step(frame: &mut ratatui::Frame, area: Rect, theme: &Theme) {
    let lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Explain is optional and off by default.",
            theme.agent_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "When enabled, synrepo can ask an LLM to produce two things:",
            theme.base_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "  1. Commentary — a short paragraph describing what a file or symbol",
            theme.base_style(),
        )),
        Line::from(Span::styled("     does. Example:", theme.base_style())),
        Line::from(Span::styled(
            format!("       \"{COMMENTARY_EXAMPLE}\""),
            theme.muted_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "  2. Cross-link candidates — proposed links between human-authored",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "     design docs and the code that implements them. Example:",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            format!("       {CROSS_LINK_EXAMPLE}"),
            theme.muted_style(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Both artifacts are stored separately from the graph, labeled as",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "machine-authored, and never silently promoted. You trigger them",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "explicitly with `synrepo sync --generate-cross-links` or the",
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "`synrepo_refresh_commentary` MCP tool — nothing runs in the background.",
            theme.base_style(),
        )),
    ];
    let block = Block::default()
        .title(" what explain does ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Draw the review screen shown after a provider was committed. Echoes the
/// user's choice back with a description + cost hint, and lists what
/// explain will and will not do in concrete terms.
pub(super) fn draw_review_explain_plan_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Review: explain plan",
        theme.agent_style(),
    )));
    lines.push(Line::from(Span::raw("")));

    match &state.explain {
        Some(ExplainChoice::Cloud {
            provider,
            credential_source,
            ..
        }) => {
            lines.push(Line::from(Span::styled(
                format!("Provider: {}", provider.config_value()),
                theme.base_style(),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", provider.description()),
                theme.muted_style(),
            )));
            lines.push(Line::from(Span::styled(
                format!("  Cost: {}", provider.cost_hint()),
                theme.muted_style(),
            )));
            lines.push(Line::from(Span::styled(
                match credential_source {
                    CloudCredentialSource::Env => format!(
                        "  Auth: use {} from the current shell.",
                        provider_env_var(*provider)
                    ),
                    CloudCredentialSource::SavedGlobal => {
                        "  Auth: reuse the saved key in ~/.synrepo/config.toml.".to_string()
                    }
                    CloudCredentialSource::EnteredGlobal => {
                        "  Auth: save the newly entered key into ~/.synrepo/config.toml on apply."
                            .to_string()
                    }
                },
                theme.muted_style(),
            )));
        }
        Some(ExplainChoice::Local { preset, endpoint }) => {
            lines.push(Line::from(Span::styled(
                format!("Provider: local ({})", preset.config_value()),
                theme.base_style(),
            )));
            lines.push(Line::from(Span::styled(
                format!("  Endpoint: {endpoint}"),
                theme.muted_style(),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", preset.description()),
                theme.muted_style(),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", preset.cost_hint()),
                theme.muted_style(),
            )));
            lines.push(Line::from(Span::styled(
                "  Endpoint persistence: save in ~/.synrepo/config.toml on apply.".to_string(),
                theme.muted_style(),
            )));
        }
        None => {
            // Should be unreachable — the state machine skips this step when
            // explain is None. Render a terse fallback so a future bug does
            // not leave a blank screen.
            lines.push(Line::from(Span::styled(
                "No provider selected — press b to go back and pick one.",
                theme.muted_style(),
            )));
        }
    }

    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "What explain will do once you run sync:",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  • write commentary on files and symbols into the overlay DB",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  • propose cross-link candidates for human review",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  • record per-call token usage and cost in `.synrepo/state/explain-*`",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "What explain will NOT do:",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  • run automatically on save, reconcile, or commit",
        theme.muted_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  • overwrite structural graph edges — candidates are curated",
        theme.muted_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  • write anything before you confirm this setup",
        theme.muted_style(),
    )));

    let block = Block::default()
        .title(" review explain plan ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
