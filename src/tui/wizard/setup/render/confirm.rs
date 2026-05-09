use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::{
    CloudCredentialSource, EmbeddingSetupChoice, ExplainChoice, SetupFlow, SetupWizardState,
};
use super::explain::provider_env_var;
use crate::tui::theme::Theme;
use crate::tui::wizard::{target_artifact_label, target_label, target_tier, AgentTargetTier};

pub(super) fn draw_confirm_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "The wizard will run the following steps:",
        theme.base_style(),
    )));
    lines.push(Line::from(Span::raw("")));
    let mut step_no: usize = 1;
    if state.flow == SetupFlow::Full {
        lines.push(Line::from(Span::styled(
            format!("  {step_no}. init .synrepo/ in {} mode", state.mode),
            theme.base_style(),
        )));
        step_no += 1;
    }
    push_gitignore_step(&mut lines, state, theme, &mut step_no);
    push_agent_steps(&mut lines, state, theme, &mut step_no);
    if state.flow == SetupFlow::Full {
        push_embedding_step(&mut lines, state, theme, step_no);
        step_no += 1;
        push_explain_step(&mut lines, state, theme, &mut step_no);
        lines.push(Line::from(Span::styled(
            format!("  {step_no}. run first reconcile pass"),
            theme.base_style(),
        )));
    }
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "No files have been written yet. Press Enter to apply or b to go back.",
        theme.muted_style(),
    )));

    let block = Block::default()
        .title(" confirm ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn push_gitignore_step(
    lines: &mut Vec<Line>,
    state: &SetupWizardState,
    theme: &Theme,
    step_no: &mut usize,
) {
    if state.add_root_gitignore {
        lines.push(Line::from(Span::styled(
            format!("  {step_no}. add .synrepo/ to root .gitignore"),
            theme.base_style(),
        )));
        *step_no += 1;
    } else if state.root_gitignore_present {
        lines.push(Line::from(Span::styled(
            "     root .gitignore already contains .synrepo/",
            theme.muted_style(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "     root .gitignore unchanged",
            theme.muted_style(),
        )));
    }
}

fn push_agent_steps(
    lines: &mut Vec<Line>,
    state: &SetupWizardState,
    theme: &Theme,
    step_no: &mut usize,
) {
    let Some(target) = state.target else {
        lines.push(Line::from(Span::styled(
            "     agent integration skipped",
            theme.muted_style(),
        )));
        return;
    };
    if state.write_agent_shim {
        lines.push(Line::from(Span::styled(
            format!(
                "  {step_no}. write {} {} (scope: project)",
                target_label(target),
                target_artifact_label(target)
            ),
            theme.base_style(),
        )));
        *step_no += 1;
    }
    if state.register_mcp {
        let mcp_line = match target_tier(target) {
            AgentTargetTier::Automated => format!(
                "  {step_no}. register MCP server for {} (scope: project)",
                target_label(target)
            ),
            AgentTargetTier::ShimOnly => format!(
                "  {step_no}. print manual MCP setup instructions for {}",
                target_label(target)
            ),
        };
        lines.push(Line::from(Span::styled(mcp_line, theme.base_style())));
        *step_no += 1;
    }
    if state.install_agent_hooks {
        lines.push(Line::from(Span::styled(
            format!(
                "  {step_no}. install local nudge hooks for {}",
                target_label(target)
            ),
            theme.base_style(),
        )));
        *step_no += 1;
    }
    if !state.write_agent_shim && !state.register_mcp && !state.install_agent_hooks {
        lines.push(Line::from(Span::styled(
            "     agent integration unchanged",
            theme.muted_style(),
        )));
    }
}

fn push_embedding_step(
    lines: &mut Vec<Line>,
    state: &SetupWizardState,
    theme: &Theme,
    step_no: usize,
) {
    let (line, style) = match state.embedding_setup {
        EmbeddingSetupChoice::Disabled => (
            format!("  {step_no}. leave embeddings disabled (optional)"),
            theme.muted_style(),
        ),
        EmbeddingSetupChoice::Onnx => (
            format!("  {step_no}. enable ONNX embeddings for semantic routing and hybrid search"),
            theme.base_style(),
        ),
        EmbeddingSetupChoice::Ollama => (
            format!("  {step_no}. enable Ollama embeddings (all-minilm at http://localhost:11434)"),
            theme.base_style(),
        ),
    };
    lines.push(Line::from(Span::styled(line, style)));
}

fn push_explain_step(
    lines: &mut Vec<Line>,
    state: &SetupWizardState,
    theme: &Theme,
    step_no: &mut usize,
) {
    let (line, style) = match &state.explain {
        Some(ExplainChoice::Cloud {
            provider,
            credential_source,
            ..
        }) => {
            let detail = match credential_source {
                CloudCredentialSource::Env => format!(
                    "use {} from the current shell",
                    provider_env_var(*provider)
                ),
                CloudCredentialSource::SavedGlobal => {
                    "reuse saved key from ~/.synrepo/config.toml".to_string()
                }
                CloudCredentialSource::EnteredGlobal => {
                    "save its API key in ~/.synrepo/config.toml".to_string()
                }
            };
            (
                format!(
                    "  {step_no}. enable explain via {} ({detail})",
                    provider.config_value()
                ),
                theme.base_style(),
            )
        }
        Some(ExplainChoice::Local { preset, endpoint }) => (
            format!(
                "  {step_no}. enable local explain ({} at {endpoint}) and save the endpoint in ~/.synrepo/config.toml",
                preset.config_value()
            ),
            theme.base_style(),
        ),
        None => (
            format!("  {step_no}. leave explain disabled (no [explain] block)"),
            theme.muted_style(),
        ),
    };
    lines.push(Line::from(Span::styled(line, style)));
    *step_no += 1;
}
