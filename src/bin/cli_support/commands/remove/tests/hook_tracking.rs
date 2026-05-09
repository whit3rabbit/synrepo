use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::json;
use synrepo::registry::{AgentHookEntry, HookEntry};
use tempfile::TempDir;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::agent_hooks::agent_hook_commands_for_tool;
use crate::cli_support::commands::hooks::full_hook_script;
use crate::cli_support::commands::step_install_agent_hooks;

use super::super::{apply_plan, build_plan, RemoveAction};
use super::Fixture;

fn isolated_home() -> (
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
    synrepo::test_support::GlobalTestLock,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempfile::tempdir().unwrap();
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard, lock)
}

fn init_git_repo(path: &Path) {
    let output = Command::new("git")
        .arg("init")
        .arg(path)
        .output()
        .expect("git init should run");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn build_plan_finds_git_hook_by_scan_and_apply_removes_it() {
    let fx = Fixture::new();
    init_git_repo(fx.path());
    let hook = fx.path().join(".git/hooks/post-commit");
    fs::write(&hook, full_hook_script()).unwrap();

    let plan = build_plan(fx.path(), None, true).unwrap();
    assert!(
        plan.actions.iter().any(|action| matches!(
            action,
            RemoveAction::RemoveGitHook { name, path, .. }
                if name == "post-commit" && path == &hook
        )),
        "scan fallback should plan a synrepo Git hook for removal"
    );

    apply_plan(fx.path(), &plan).unwrap();
    assert!(!hook.exists(), "full-file hook should be deleted");
}

#[test]
fn apply_agent_hook_removal_preserves_user_hooks_in_same_group() {
    let fx = Fixture::new();
    let path = fx.path().join(".codex/hooks.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let [prompt, pre_tool] = agent_hook_commands_for_tool(AgentTool::Codex).unwrap();
    fs::write(
        &path,
        serde_json::to_string_pretty(&json!({
            "permissions": { "allow": ["Bash(git diff:*)"] },
            "hooks": {
                "UserPromptSubmit": [{
                    "hooks": [
                        { "type": "command", "command": prompt, "timeout": 5 },
                        { "type": "command", "command": "echo user", "timeout": 5 }
                    ]
                }],
                "PreToolUse": [{
                    "matcher": "Bash|apply_patch",
                    "hooks": [
                        { "type": "command", "command": pre_tool, "timeout": 5 },
                        { "type": "command", "command": "echo pre", "timeout": 5 }
                    ]
                }]
            }
        }))
        .unwrap(),
    )
    .unwrap();

    super::super::hook_artifacts::remove_agent_hook("codex", &path).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    assert!(raw.contains("echo user"));
    assert!(raw.contains("echo pre"));
    assert!(raw.contains("Bash(git diff:*)"));
    assert!(!raw.contains("synrepo agent-hook nudge"));
}

#[test]
fn finalize_remove_clears_tracked_hooks_only_after_success() {
    let (_home, _guard, _lock) = isolated_home();
    let project = TempDir::new().unwrap();
    synrepo::registry::record_install(project.path(), true).unwrap();
    step_install_agent_hooks(project.path(), AgentTool::Codex).unwrap();
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

    let plan = build_plan(project.path(), None, true).unwrap();
    super::super::finalize_remove(project.path(), None, &plan, true).unwrap();

    let entry = synrepo::registry::get(project.path()).unwrap().unwrap();
    assert!(entry.hooks.is_empty());
    assert!(entry.agent_hooks.is_empty());
    assert!(
        entry.root_gitignore_entry_added,
        "unrelated project ownership must survive"
    );
}

#[test]
fn finalize_remove_preserves_hook_record_when_action_fails() {
    let (_home, _guard, _lock) = isolated_home();
    let project = TempDir::new().unwrap();
    let hook_path = project.path().join("bad-hook");
    fs::create_dir_all(&hook_path).unwrap();
    synrepo::registry::record_hooks(
        project.path(),
        vec![HookEntry {
            name: "post-commit".to_string(),
            path: "bad-hook".to_string(),
            mode: "full_file".to_string(),
            installed_at: "2026-04-29T00:00:00Z".to_string(),
        }],
    )
    .unwrap();
    synrepo::registry::record_agent_hooks(
        project.path(),
        vec![AgentHookEntry {
            tool: "codex".to_string(),
            path: ".codex/hooks.json".to_string(),
            installed_at: "2026-04-29T00:00:00Z".to_string(),
        }],
    )
    .unwrap();

    let plan = build_plan(project.path(), None, true).unwrap();
    super::super::finalize_remove(project.path(), None, &plan, true).unwrap();

    let entry = synrepo::registry::get(project.path()).unwrap().unwrap();
    assert_eq!(entry.hooks.len(), 1);
    assert_eq!(entry.agent_hooks.len(), 1);
}
