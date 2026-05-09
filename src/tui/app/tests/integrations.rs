use super::super::*;
use super::support::make_poll_state;
use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::tui::agent_integrations::AgentInstallDisplayRow;
use crate::tui::probe::Severity;
use crossterm::event::{KeyCode, KeyModifiers};

fn integration_row(tool: &str, agent: &str) -> AgentInstallDisplayRow {
    AgentInstallDisplayRow {
        tool: tool.to_string(),
        agent: agent.to_string(),
        overall_label: "missing",
        overall_severity: Severity::Stale,
        context: "skill missing missing not detected".to_string(),
        context_severity: Severity::Stale,
        mcp: "mcp missing missing not detected".to_string(),
        mcp_severity: Severity::Stale,
        hooks: "unsupported unsupported".to_string(),
        hooks_severity: Severity::Healthy,
        next_action: "synrepo setup codex --project".to_string(),
    }
}

#[test]
fn integrations_tab_moves_selection_and_enter_launches_seeded_wizard() {
    let mut state = make_poll_state();
    state.integration_display_rows = vec![
        integration_row("claude", "Claude Code"),
        integration_row("codex", "Codex CLI"),
    ];
    state.set_tab(ActiveTab::Mcp);

    assert!(state.handle_key(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(state.integration_selected_index(), 1);
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(state.should_exit);
    assert_eq!(
        state.exit_intent(),
        DashboardExit::LaunchIntegration(IntegrationLaunchRequest {
            initial_target: Some(AgentTargetKind::Codex),
        })
    );
}

#[test]
fn integrations_tab_selection_clamps_and_preserves_tool() {
    let mut state = make_poll_state();
    state.integration_display_rows = vec![
        integration_row("claude", "Claude Code"),
        integration_row("codex", "Codex CLI"),
    ];
    state.integration_selected = 1;
    state.integration_display_rows = vec![
        integration_row("codex", "Codex CLI"),
        integration_row("claude", "Claude Code"),
    ];
    state.preserve_integration_selection(Some("codex"));
    assert_eq!(state.integration_selected_index(), 0);

    state.integration_selected = 99;
    state.integration_display_rows = vec![integration_row("claude", "Claude Code")];
    state.preserve_integration_selection(None);
    assert_eq!(state.integration_selected_index(), 0);
}

#[test]
fn integrations_tab_enter_on_unknown_tool_stays_in_dashboard() {
    let mut state = make_poll_state();
    state.integration_display_rows = vec![integration_row("custom", "Custom Agent")];
    state.set_tab(ActiveTab::Mcp);

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(!state.should_exit);
    assert!(state.launch_integration.is_none());
    assert!(state
        .active_toast()
        .is_some_and(|toast| toast.contains("no integration wizard target")));
}
