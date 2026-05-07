use std::fs;
use std::path::Path;

use synrepo::tui::McpInstallPlan;
use tempfile::tempdir;

use super::apply::apply_uninstall_plan;
use super::plan::{
    build_uninstall_plan, PlannedAction, ProjectUninstallPlan, UninstallAction, UninstallPlan,
};
use crate::cli_support::commands::hooks::{full_hook_script, marked_hook_block};
use crate::cli_support::commands::remove::RemoveAction;
use crate::cli_support::repair_cmd::execute_project_mcp_install_plan;
use synrepo::registry::HookEntry;

fn isolated_home() -> (
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
    synrepo::test_support::GlobalTestLock,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard, lock)
}

fn record_project_with_data(project: &Path) {
    fs::create_dir_all(project.join(".synrepo")).unwrap();
    fs::write(project.join(".gitignore"), ".synrepo/\n").unwrap();
    synrepo::registry::record_install(project, true).unwrap();
}

#[test]
fn planner_keeps_project_and_global_data_by_default() {
    let (_home, _guard, _lock) = isolated_home();
    let project = tempdir().unwrap();
    record_project_with_data(project.path());

    let plan = build_uninstall_plan(project.path(), false, true).unwrap();
    let project_actions = &plan.projects[0].actions;
    assert!(project_actions.iter().any(|item| matches!(
        item.action,
        UninstallAction::DeleteProjectSynrepoDir { .. }
    ) && !item.enabled));
    assert!(project_actions.iter().any(|item| matches!(
        item.action,
        UninstallAction::RemoveProjectGitignoreLine { .. }
    ) && !item.enabled));
    assert!(plan.global.iter().any(|item| matches!(
        item.action,
        UninstallAction::DeleteGlobalSynrepoDir { .. }
    ) && !item.enabled));
}

#[test]
fn delete_data_selects_data_and_gitignore_rows() {
    let (_home, _guard, _lock) = isolated_home();
    let project = tempdir().unwrap();
    record_project_with_data(project.path());

    let plan = build_uninstall_plan(project.path(), true, true).unwrap();
    let actions = &plan.projects[0].actions;
    assert!(actions.iter().any(|item| matches!(
        item.action,
        UninstallAction::DeleteProjectSynrepoDir { .. }
    ) && item.enabled));
    assert!(actions.iter().any(|item| matches!(
        item.action,
        UninstallAction::RemoveProjectGitignoreLine { .. }
    ) && item.enabled));
}

#[test]
fn missing_registry_projects_are_reported_as_skipped() {
    let (_home, _guard, _lock) = isolated_home();
    let current = tempdir().unwrap();
    let missing = current.path().join("missing");
    synrepo::registry::record_install(&missing, false).unwrap();

    let plan = build_uninstall_plan(current.path(), false, true).unwrap();
    assert!(plan
        .skipped
        .iter()
        .any(|item| item.target == missing.display().to_string()));
}

#[test]
fn planner_dedupes_global_agent_artifacts_across_projects() {
    let (home, _guard, _lock) = isolated_home();
    let project_a = tempdir().unwrap();
    let project_b = tempdir().unwrap();
    let shim = home.path().join(".claude/skills/synrepo/SKILL.md");
    fs::create_dir_all(shim.parent().unwrap()).unwrap();
    fs::write(&shim, "# synrepo\n").unwrap();
    for project in [project_a.path(), project_b.path()] {
        synrepo::registry::record_agent(
            project,
            synrepo::registry::AgentEntry {
                tool: "claude".to_string(),
                scope: "global".to_string(),
                shim_path: shim.to_string_lossy().into_owned(),
                mcp_config_path: None,
                mcp_backup_path: None,
                installed_at: "2026-04-29T00:00:00Z".to_string(),
            },
        )
        .unwrap();
    }

    let plan = build_uninstall_plan(project_a.path(), false, true).unwrap();
    let count = plan
        .projects
        .iter()
        .flat_map(|project| &project.actions)
        .filter(|item| matches!(
            &item.action,
            UninstallAction::ProjectRemove {
                remove_action: crate::cli_support::commands::remove::RemoveAction::DeleteShim { path, .. },
                ..
            } if path == &shim
        ))
        .count();
    assert_eq!(count, 1, "global shim should only be planned once");
}

#[test]
fn planner_includes_paired_dashboard_mcp_install_for_full_uninstall() {
    let (_home, _guard, _lock) = isolated_home();
    let project = tempdir().unwrap();

    execute_project_mcp_install_plan(
        project.path(),
        McpInstallPlan {
            target: "claude".to_string(),
        },
    )
    .unwrap();

    let plan = build_uninstall_plan(project.path(), false, true).unwrap();
    let actions = plan
        .projects
        .iter()
        .find(|p| p.path == synrepo::registry::canonicalize_path(project.path()))
        .expect("project plan")
        .actions
        .iter()
        .filter(|item| item.enabled)
        .map(|item| &item.action)
        .collect::<Vec<_>>();
    assert!(
        actions.iter().any(|action| matches!(
            action,
            UninstallAction::ProjectRemove {
                remove_action: RemoveAction::DeleteShim { tool, .. },
                ..
            } if tool == "claude"
        )),
        "full uninstall should include paired skill removal"
    );
    assert!(
        actions.iter().any(|action| matches!(
            action,
            UninstallAction::ProjectRemove {
                remove_action: RemoveAction::StripMcpEntry { tool, .. },
                ..
            } if tool == "claude"
        )),
        "full uninstall should include paired MCP removal"
    );
}

#[test]
fn applying_hook_keeps_project_data_tracking_until_data_is_deleted() {
    let (_home, _guard, _lock) = isolated_home();
    let project = tempdir().unwrap();
    record_project_with_data(project.path());
    let hook_path = project.path().join(".git/hooks/post-commit");
    fs::create_dir_all(hook_path.parent().unwrap()).unwrap();
    fs::write(&hook_path, full_hook_script()).unwrap();
    synrepo::registry::record_hooks(
        project.path(),
        vec![HookEntry {
            name: "post-commit".to_string(),
            path: ".git/hooks/post-commit".to_string(),
            mode: "full_file".to_string(),
            installed_at: "2026-04-29T00:00:00Z".to_string(),
        }],
    )
    .unwrap();

    let plan = build_uninstall_plan(project.path(), false, true).unwrap();
    apply_uninstall_plan(&plan, false).unwrap();

    let entry = synrepo::registry::get(project.path()).unwrap().unwrap();
    assert!(entry.root_gitignore_entry_added);
    assert!(entry.hooks.is_empty());
    assert!(project.path().join(".synrepo").exists());
}

#[test]
fn deleting_project_data_clears_final_registry_entry() {
    let (_home, _guard, _lock) = isolated_home();
    let project = tempdir().unwrap();
    record_project_with_data(project.path());

    let mut plan = build_uninstall_plan(project.path(), true, true).unwrap();
    plan.global.clear();
    apply_uninstall_plan(&plan, false).unwrap();

    assert!(synrepo::registry::get(project.path()).unwrap().is_none());
    assert!(!project.path().join(".synrepo").exists());
    let gitignore = fs::read_to_string(project.path().join(".gitignore")).unwrap_or_default();
    assert!(!gitignore.lines().any(|line| line.trim() == ".synrepo/"));
}

#[test]
fn applying_full_file_hook_removes_the_hook_file() {
    let (_home, _guard, _lock) = isolated_home();
    let hook = tempdir().unwrap();
    let hook_path = hook.path().join("post-commit");
    fs::write(&hook_path, full_hook_script()).unwrap();
    let plan = one_hook_plan(&hook_path, "full_file");

    let summary = apply_uninstall_plan(&plan, false).unwrap();
    assert!(summary.applied[0].succeeded);
    assert!(!hook_path.exists());
}

#[test]
fn applying_marked_hook_preserves_user_content() {
    let (_home, _guard, _lock) = isolated_home();
    let hook = tempdir().unwrap();
    let hook_path = hook.path().join("post-merge");
    fs::write(
        &hook_path,
        format!(
            "#!/bin/sh\necho before\n{}\necho after\n",
            marked_hook_block()
        ),
    )
    .unwrap();
    let plan = one_hook_plan(&hook_path, "marked_block");

    let summary = apply_uninstall_plan(&plan, false).unwrap();
    assert!(summary.applied[0].succeeded);
    let raw = fs::read_to_string(&hook_path).unwrap();
    assert!(raw.contains("echo before"));
    assert!(raw.contains("echo after"));
    assert!(!raw.contains("synrepo reconcile"));
}

#[test]
fn applying_legacy_hook_strips_only_synrepo_lines() {
    let (_home, _guard, _lock) = isolated_home();
    let hook = tempdir().unwrap();
    let hook_path = hook.path().join("post-checkout");
    fs::write(
        &hook_path,
        "#!/bin/sh\necho before\n# synrepo hook\n(synrepo reconcile --fast > /dev/null 2>&1 &)\necho after\n",
    )
    .unwrap();
    let plan = one_hook_plan(&hook_path, "legacy");

    let summary = apply_uninstall_plan(&plan, false).unwrap();
    assert!(summary.applied[0].succeeded);
    let raw = fs::read_to_string(&hook_path).unwrap();
    assert!(raw.contains("echo before"));
    assert!(raw.contains("echo after"));
    assert!(!raw.contains("synrepo reconcile"));
}

fn one_hook_plan(hook_path: &Path, mode: &str) -> UninstallPlan {
    let project = hook_path.parent().unwrap().to_path_buf();
    UninstallPlan {
        projects: vec![ProjectUninstallPlan {
            path: project.clone(),
            exists: true,
            preserved: Vec::new(),
            actions: vec![PlannedAction {
                action: UninstallAction::RemoveHook {
                    project,
                    name: "post-commit".to_string(),
                    path: hook_path.to_path_buf(),
                    mode: mode.to_string(),
                },
                enabled: true,
                destructive: false,
            }],
        }],
        global: Vec::new(),
        preserved: Vec::new(),
        skipped: Vec::new(),
        manual_followups: Vec::new(),
    }
}
