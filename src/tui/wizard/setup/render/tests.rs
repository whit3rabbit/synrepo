use ratatui::backend::TestBackend;
use ratatui::Terminal;

use super::draw;
use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::config::Mode;
use crate::tui::theme::Theme;
use crate::tui::wizard::setup::state::{EmbeddingSetupChoice, SetupStep, SetupWizardState};

fn render_state(state: &SetupWizardState) -> String {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| draw(frame, state, &Theme::plain()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for row in buffer.content().chunks(buffer.area.width as usize) {
        for cell in row {
            text.push_str(cell.symbol());
        }
        text.push('\n');
    }
    text
}

#[test]
fn first_run_choice_steps_are_single_selection_lists() {
    let mut state = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);

    for step in [
        SetupStep::SelectMode,
        SetupStep::SelectTarget,
        SetupStep::SelectEmbeddings,
        SetupStep::SelectExplain,
    ] {
        state.step = step;
        let screen = render_state(&state);
        assert!(
            screen.contains("Enter select"),
            "{step:?} should select one row"
        );
        assert!(
            !screen.contains("[ ]") && !screen.contains("[x]"),
            "{step:?} must not render checkbox markers"
        );
        assert!(
            !screen.contains("Space toggle"),
            "{step:?} must not imply multi-select behavior"
        );
    }
}

#[test]
fn first_run_confirm_lists_concrete_mcp_setup_plan() {
    let mut state = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);
    state.step = SetupStep::Confirm;
    state.mode = Mode::Auto;
    state.target = Some(AgentTargetKind::Claude);
    state.explain = None;

    let screen = render_state(&state);

    assert!(screen.contains("init .synrepo/ in auto mode"));
    assert!(screen.contains("add .synrepo/ to root .gitignore"));
    assert!(screen.contains("write Claude Code skill"));
    assert!(screen.contains("scope: project"));
    assert!(screen.contains("register MCP server for Claude Code"));
    assert!(screen.contains("leave embeddings disabled"));
    assert!(screen.contains("leave explain disabled"));
    assert!(screen.contains("No files have been written yet"));
}

#[test]
fn actions_step_renders_gitignore_and_hook_toggles() {
    let mut state = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Codex]);
    state.step = SetupStep::SelectActions;

    let screen = render_state(&state);

    assert!(screen.contains("[x] Add .synrepo/ to root .gitignore"));
    assert!(screen.contains("[x] Write or update Codex CLI skill"));
    assert!(screen.contains("[x] Register repo-local MCP server for Codex CLI"));
    assert!(screen.contains("[ ] Install local nudge hooks for Codex CLI"));
    assert!(screen.contains("Space toggle"));
}

#[test]
fn embeddings_step_names_provider_choices() {
    let mut state = SetupWizardState::new(Mode::Auto, vec![]);
    state.step = SetupStep::SelectEmbeddings;

    let screen = render_state(&state);

    assert!(screen.contains("Skip"));
    assert!(screen.contains("ONNX"));
    assert!(screen.contains("Ollama"));
}

#[test]
fn confirm_names_selected_embedding_provider() {
    let mut state = SetupWizardState::new(Mode::Auto, vec![]);
    state.step = SetupStep::Confirm;
    state.embedding_setup = EmbeddingSetupChoice::Ollama;

    let screen = render_state(&state);

    assert!(screen.contains("enable Ollama embeddings"));
}
