use std::path::PathBuf;

use tempfile::tempdir;

use super::*;
use crate::config::test_home::HomeEnvGuard;
use crate::pipeline::writer::now_rfc3339;
use crate::registry::AgentEntry;
use crate::tui::probe::Severity;

fn home_guard() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    HomeEnvGuard,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = HomeEnvGuard::redirect_to(home.path());
    (lock, home, guard)
}

fn agent(tool: &str, scope: &str, path: Option<&str>) -> AgentEntry {
    AgentEntry {
        tool: tool.to_string(),
        scope: scope.to_string(),
        shim_path: "shim".to_string(),
        mcp_config_path: path.map(str::to_string),
        mcp_backup_path: None,
        installed_at: now_rfc3339(),
    }
}

fn row<'a>(rows: &'a [McpStatusRow], tool: &str) -> &'a McpStatusRow {
    rows.iter()
        .find(|row| row.tool == tool)
        .unwrap_or_else(|| panic!("missing row for {tool}: {rows:#?}"))
}

#[test]
fn registry_global_entry_marks_global_scope() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    registry::record_agent(
        repo.path(),
        agent("claude", "global", Some("/tmp/claude.json")),
    )
    .unwrap();

    let rows = build_mcp_status_rows(repo.path());
    let row = row(&rows, "claude");

    assert_eq!(row.status, McpStatus::Registered);
    assert_eq!(row.scope, McpScope::Global);
    assert_eq!(row.source, "registry record");
}

#[test]
fn registry_project_entry_marks_project_scope() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    registry::record_agent(
        repo.path(),
        agent("codex", "project", Some(".codex/config.toml")),
    )
    .unwrap();

    let rows = build_mcp_status_rows(repo.path());
    let row = row(&rows, "codex");

    assert_eq!(row.status, McpStatus::Registered);
    assert_eq!(row.scope, McpScope::Project);
    assert!(row
        .config_path
        .as_ref()
        .unwrap()
        .ends_with(".codex/config.toml"));
}

#[test]
fn missing_config_marks_mcp_capable_agent_missing() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();

    let rows = build_mcp_status_rows(repo.path());
    let row = row(&rows, "claude");

    assert_eq!(row.status, McpStatus::Missing);
    assert_eq!(row.scope, McpScope::Missing);
}

#[test]
fn unsupported_registry_tool_is_reported() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    registry::record_agent(repo.path(), agent("generic", "project", None)).unwrap();

    let rows = build_mcp_status_rows(repo.path());
    let row = row(&rows, "generic");

    assert_eq!(row.status, McpStatus::Unsupported);
    assert_eq!(row.scope, McpScope::Unsupported);
    assert_eq!(row.source, "registry record");
}

#[test]
fn legacy_claude_config_falls_back_to_project_scope() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"synrepo":{"command":"synrepo","args":["mcp","--repo","."]}}}"#,
    )
    .unwrap();

    let rows = build_mcp_status_rows(repo.path());
    let row = row(&rows, "claude");

    assert_eq!(row.status, McpStatus::Registered);
    assert_eq!(row.scope, McpScope::Project);
    assert_eq!(row.source, "legacy config");
}

#[test]
fn display_rows_preformat_path_and_fixed_cells() {
    let row = McpStatusRow {
        agent: "Codex CLI".to_string(),
        tool: "codex".to_string(),
        status: McpStatus::Registered,
        scope: McpScope::Project,
        source: "legacy config".to_string(),
        config_path: Some(PathBuf::from(".codex/config.toml")),
    };

    let rows = build_mcp_display_rows(&[row]);

    assert_eq!(rows[0].agent_cell, "Codex CLI         ");
    assert_eq!(rows[0].status_label, "registered");
    assert_eq!(rows[0].status_severity, Severity::Healthy);
    assert_eq!(rows[0].scope_cell, " scope:project    ");
    assert_eq!(rows[0].source_cell, " source:legacy config     ");
    assert_eq!(rows[0].path_cell, " .codex/config.toml");
}
