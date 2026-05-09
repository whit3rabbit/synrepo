use std::fs;

use synrepo::bootstrap::runtime_probe::AgentTargetKind;
use synrepo::tui::{IntegrationPlan, McpInstallPlan};
use tempfile::tempdir;

use crate::cli_support::agent_shims::{AgentTool, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::cli_support::commands::{remove_apply_plan, remove_build_plan, RemoveAction};
use crate::cli_support::repair_cmd::{execute_integration_plan, execute_project_mcp_install_plan};

fn has_line(report: &crate::cli_support::apply_report::ApplyReport, expected: &str) -> bool {
    report.lines().iter().any(|line| line == expected)
}

fn isolated_home() -> (
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
    synrepo::test_support::GlobalTestLock,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = crate::cli_support::tests::support::canonicalize_no_verbatim(home.path());
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    (home, guard, lock)
}

#[test]
fn claude_dashboard_mcp_install_pairs_repo_local_mcp_with_skill() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = tempdir().unwrap();
    let repo_root = crate::cli_support::tests::support::canonicalize_no_verbatim(repo.path());

    let report = execute_project_mcp_install_plan(
        &repo_root,
        McpInstallPlan {
            target: "claude".to_string(),
        },
    )
    .unwrap();
    assert!(has_line(&report, "Shim: applied"));
    assert!(report.lines().iter().any(|line| line.starts_with("MCP: ")));
    assert!(report
        .lines()
        .iter()
        .any(|line| line.starts_with("Backup: ")));

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
    assert_eq!(
        parsed["mcpServers"]["synrepo"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp", "--repo", "."],
        "dashboard MCP tab install must use repo-local command args"
    );
    assert!(
        repo_root
            .join(".claude")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md")
            .exists(),
        "dashboard MCP install must pair Claude MCP with the project skill"
    );
}

#[test]
fn integration_plan_mcp_only_still_writes_skill() {
    let (home, _guard, _lock) = isolated_home();
    let home_root = crate::cli_support::tests::support::canonicalize_no_verbatim(home.path());
    let repo = tempdir().unwrap();
    let repo_root = crate::cli_support::tests::support::canonicalize_no_verbatim(repo.path());

    let report = execute_integration_plan(
        &repo_root,
        IntegrationPlan {
            target: AgentTargetKind::Claude,
            write_shim: false,
            register_mcp: true,
            overwrite_shim: false,
            install_agent_hooks: false,
        },
    )
    .unwrap();
    assert!(has_line(&report, "Shim: applied"));
    assert!(report.lines().iter().any(|line| line.starts_with("MCP: ")));
    assert!(has_line(&report, "Hooks: unchanged"));

    assert!(
        home_root
            .join(".claude")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md")
            .exists(),
        "global MCP registration must ensure the paired global Claude skill"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(home_root.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}

#[test]
fn integration_plan_reports_agent_hooks_applied_and_current() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = tempdir().unwrap();
    let repo_root = crate::cli_support::tests::support::canonicalize_no_verbatim(repo.path());
    let plan = IntegrationPlan {
        target: AgentTargetKind::Claude,
        write_shim: false,
        register_mcp: false,
        overwrite_shim: false,
        install_agent_hooks: true,
    };

    let first = execute_integration_plan(&repo_root, plan.clone()).unwrap();
    assert!(has_line(&first, "Shim: unchanged"));
    assert!(has_line(&first, "MCP: skipped"));
    assert!(has_line(&first, "Hooks: applied"));

    let second = execute_integration_plan(&repo_root, plan).unwrap();
    assert!(has_line(&second, "Hooks: already current"));
}

#[test]
fn project_mcp_install_reports_current_second_run() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = tempdir().unwrap();
    let repo_root = crate::cli_support::tests::support::canonicalize_no_verbatim(repo.path());
    let plan = McpInstallPlan {
        target: "claude".to_string(),
    };

    execute_project_mcp_install_plan(&repo_root, plan.clone()).unwrap();
    let second = execute_project_mcp_install_plan(&repo_root, plan).unwrap();

    assert!(has_line(&second, "Shim: already current"));
    assert!(has_line(&second, "MCP: already current"));
    assert!(second
        .lines()
        .iter()
        .any(|line| line.starts_with("Backup: ")));
}

#[test]
fn remove_paired_dashboard_mcp_install_removes_skill_and_mcp() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = tempdir().unwrap();
    let repo_root = crate::cli_support::tests::support::canonicalize_no_verbatim(repo.path());

    execute_project_mcp_install_plan(
        &repo_root,
        McpInstallPlan {
            target: "claude".to_string(),
        },
    )
    .unwrap();

    let plan = remove_build_plan(&repo_root, Some(AgentTool::Claude), false).unwrap();
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, RemoveAction::DeleteShim { tool, .. } if tool == "claude")),
        "paired dashboard install should plan skill removal"
    );
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, RemoveAction::StripMcpEntry { tool, .. } if tool == "claude")),
        "paired dashboard install should plan MCP removal"
    );

    remove_apply_plan(&repo_root, &plan).unwrap();
    let scope = agent_config::Scope::Local(repo_root);
    let skill = agent_config::skill_by_id("claude").unwrap();
    let mcp = agent_config::mcp_by_id("claude").unwrap();
    assert!(matches!(
        skill
            .skill_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .unwrap()
            .status,
        agent_config::InstallStatus::Absent
    ));
    assert!(matches!(
        mcp.mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .unwrap()
            .status,
        agent_config::InstallStatus::Absent
    ));
}
