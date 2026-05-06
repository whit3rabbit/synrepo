use std::fs;

use crossterm::event::{KeyCode, KeyModifiers};
use tempfile::tempdir;

use super::state::*;
use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::tui::mcp_status::{build_mcp_status_rows, McpScope, McpStatus, McpStatusRow};

fn row(tool: &str, status: McpStatus, source: &str) -> McpStatusRow {
    let agent = match tool {
        "claude" => "Claude Code",
        "codex" => "Codex CLI",
        "cursor" => "Cursor",
        "copilot" => "GitHub Copilot",
        "windsurf" => "Windsurf",
        _ => tool,
    };
    let scope = if status == McpStatus::Registered {
        McpScope::Project
    } else {
        McpScope::Missing
    };
    McpStatusRow {
        agent: agent.to_string(),
        tool: tool.to_string(),
        status,
        scope,
        source: source.to_string(),
        config_path: None,
    }
}

#[test]
fn defaults_to_detected_missing_local_mcp_target() {
    let repo = tempdir().unwrap();
    let rows = vec![
        row("claude", McpStatus::Missing, "not detected"),
        row("codex", McpStatus::Missing, "target hint"),
    ];

    let state = McpInstallWizardState::new(repo.path(), rows, vec![AgentTargetKind::Codex]);

    assert_eq!(state.target, "codex");
    assert_eq!(state.selected_row().unwrap().status, McpStatus::Missing);
}

#[test]
fn produces_plan_for_selected_target() {
    let repo = tempdir().unwrap();
    let rows = vec![
        row("claude", McpStatus::Missing, "not detected"),
        row("codex", McpStatus::Missing, "target hint"),
    ];
    let mut state = McpInstallWizardState::new(repo.path(), rows, vec![AgentTargetKind::Claude]);

    state.handle_key(KeyCode::Down, KeyModifiers::NONE);
    state.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    state.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(
        state.finalize(),
        Some(McpInstallPlan {
            target: "codex".to_string()
        })
    );
}

#[test]
fn registered_targets_can_be_selected_for_idempotent_rerun() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".codex")).unwrap();
    fs::write(
        repo.path().join(".codex").join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n",
    )
    .unwrap();
    let rows = build_mcp_status_rows(repo.path());
    let mut state = McpInstallWizardState::new(repo.path(), rows, vec![AgentTargetKind::Codex]);

    while state.target != "codex" {
        state.handle_key(KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(state.selected_row().unwrap().status, McpStatus::Registered);

    state.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    state.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(
        state.finalize(),
        Some(McpInstallPlan {
            target: "codex".to_string()
        })
    );
}

#[test]
fn picker_includes_every_local_agent_config_mcp_target() {
    let repo = tempdir().unwrap();
    let rows = build_mcp_status_rows(repo.path());
    let state = McpInstallWizardState::new(repo.path(), rows, vec![]);
    let actual: std::collections::HashSet<_> =
        state.rows().iter().map(|row| row.target.as_str()).collect();
    let expected: std::collections::HashSet<_> = agent_config::mcp_capable()
        .into_iter()
        .filter(|installer| {
            installer
                .supported_mcp_scopes()
                .contains(&agent_config::ScopeKind::Local)
        })
        .map(|installer| installer.id())
        .collect();
    assert_eq!(actual, expected);
}
