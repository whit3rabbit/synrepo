use tempfile::tempdir;

use super::*;
use crate::pipeline::writer::now_rfc3339;
use crate::registry::AgentEntry;

fn isolated_home() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (lock, home, guard)
}

fn row<'a>(rows: &'a [AgentInstallStatus], tool: &str) -> &'a AgentInstallStatus {
    rows.iter()
        .find(|row| row.tool == tool)
        .unwrap_or_else(|| panic!("missing row for {tool}: {rows:#?}"))
}

fn install_codex_skill_and_mcp(repo: &std::path::Path) {
    let scope = agent_config::Scope::Local(repo.to_path_buf());
    let skill = agent_config::skill_by_id("codex").unwrap();
    let skill_spec = agent_config::SkillSpec::builder("synrepo")
        .owner("synrepo")
        .description("Use when a repository has synrepo context available.")
        .body("# Synrepo\n")
        .build();
    let _ = skill.install_skill(&scope, &skill_spec).unwrap();

    let mcp = agent_config::mcp_by_id("codex").unwrap();
    let mcp_spec = agent_config::McpSpec::builder("synrepo")
        .owner("synrepo")
        .stdio("synrepo", ["mcp", "--repo", "."])
        .build();
    let _ = mcp.install_mcp(&scope, &mcp_spec).unwrap();
}

#[test]
fn complete_install_ignores_missing_optional_hooks() {
    let (_lock, _home, _guard) = isolated_home();
    let repo = tempdir().unwrap();
    install_codex_skill_and_mcp(repo.path());

    let rows = build_agent_install_statuses(repo.path());
    let codex = row(&rows, "codex");

    assert_eq!(codex.context.status, ComponentStatus::Installed);
    assert_eq!(codex.context.scope, InstallScope::Project);
    assert_eq!(codex.mcp.status, ComponentStatus::Installed);
    assert_eq!(codex.hooks.status, HookStatus::Missing);
    assert_eq!(codex.overall, AgentOverallStatus::Complete);
    assert!(codex.next_action.contains("--agent-hooks"));
}

#[test]
fn legacy_mcp_without_context_is_partial() {
    let (_lock, _home, _guard) = isolated_home();
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"synrepo":{"command":"synrepo","args":["mcp","--repo","."]}}}"#,
    )
    .unwrap();

    let rows = build_agent_install_statuses(repo.path());
    let claude = row(&rows, "claude");

    assert_eq!(claude.context.status, ComponentStatus::Missing);
    assert_eq!(claude.mcp.status, ComponentStatus::Installed);
    assert_eq!(claude.mcp.source, "legacy config");
    assert_eq!(claude.overall, AgentOverallStatus::Partial);
}

#[test]
fn registry_unknown_tool_reports_unsupported() {
    let (_lock, _home, _guard) = isolated_home();
    let repo = tempdir().unwrap();
    crate::registry::record_agent(
        repo.path(),
        AgentEntry {
            tool: "generic".to_string(),
            scope: "project".to_string(),
            shim_path: "".to_string(),
            mcp_config_path: None,
            mcp_backup_path: None,
            installed_at: now_rfc3339(),
        },
    )
    .unwrap();

    let rows = build_agent_install_statuses(repo.path());
    let generic = row(&rows, "generic");

    assert_eq!(generic.context.status, ComponentStatus::Unsupported);
    assert_eq!(generic.mcp.status, ComponentStatus::Unsupported);
    assert_eq!(generic.hooks.status, HookStatus::Unsupported);
    assert_eq!(generic.overall, AgentOverallStatus::Unsupported);
}

#[test]
fn codex_hooks_are_detected_from_project_config() {
    let (_lock, _home, _guard) = isolated_home();
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join(".codex")).unwrap();
    std::fs::write(
        repo.path().join(".codex/hooks.json"),
        r#"{
          "hooks": {
            "UserPromptSubmit": [{"hooks": [{"command": "synrepo agent-hook nudge --client codex --event UserPromptSubmit"}]}],
            "PreToolUse": [{"hooks": [{"command": "synrepo agent-hook nudge --client codex --event PreToolUse"}]}]
          }
        }"#,
    )
    .unwrap();

    let rows = build_agent_install_statuses(repo.path());
    let codex = row(&rows, "codex");

    assert_eq!(codex.hooks.status, HookStatus::Installed);
    assert_eq!(codex.hooks.source, "hook config");
}
