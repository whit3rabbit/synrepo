//! Tests for `synrepo remove` plan building and application.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::json;
use synrepo::registry::{AgentHookEntry, HookEntry};
use tempfile::TempDir;

use crate::cli_support::agent_shims::{AgentTool, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::cli_support::commands::agent_hooks::agent_hook_commands_for_tool;
use crate::cli_support::commands::hooks::full_hook_script;
use crate::cli_support::commands::step_install_agent_hooks;

use super::{apply_plan, build_plan, RemoveAction};

/// Repo-only test fixture. We intentionally do NOT re-point HOME, because
/// `std::env::set_var` is process-global and parallel tests would race. Every
/// test here only needs scan-based detection (no registry-driven plan), so
/// the real `~/.synrepo/projects.toml` is irrelevant: `registry::get` matches
/// on canonicalized paths, and the tempdir never appears in the real registry.
struct Fixture {
    repo: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            repo: TempDir::new().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.repo.path()
    }
}

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

fn write_claude_shim(repo: &Path) {
    let dir = repo.join(".claude").join("skills").join("synrepo");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), b"# test shim\n").unwrap();
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

    super::hook_artifacts::remove_agent_hook("codex", &path).unwrap();

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
    super::finalize_remove(project.path(), None, &plan, true).unwrap();

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
    super::finalize_remove(project.path(), None, &plan, true).unwrap();

    let entry = synrepo::registry::get(project.path()).unwrap().unwrap();
    assert_eq!(entry.hooks.len(), 1);
    assert_eq!(entry.agent_hooks.len(), 1);
}

fn write_mcp_json_with_synrepo(repo: &Path, extra_server: Option<(&str, serde_json::Value)>) {
    let mut servers = serde_json::Map::new();
    servers.insert(
        "synrepo".to_string(),
        json!({
            "command": "synrepo",
            "args": ["mcp", "--repo", "."],
            "scope": "project",
        }),
    );
    if let Some((k, v)) = extra_server {
        servers.insert(k.to_string(), v);
    }
    let value = json!({ "mcpServers": servers });
    fs::write(
        repo.join(".mcp.json"),
        serde_json::to_string_pretty(&value).unwrap(),
    )
    .unwrap();
}

#[test]
fn build_plan_empty_repo_yields_no_actions() {
    let fx = Fixture::new();
    let plan = build_plan(fx.path(), None, false).unwrap();
    assert!(plan.is_empty(), "empty repo should produce an empty plan");
}

#[test]
fn build_plan_finds_shim_and_mcp_entry_by_scan() {
    let fx = Fixture::new();
    write_claude_shim(fx.path());
    write_mcp_json_with_synrepo(fx.path(), None);

    let plan = build_plan(fx.path(), None, false).unwrap();
    let has_shim = plan
        .actions
        .iter()
        .any(|a| matches!(a, RemoveAction::DeleteShim { tool, .. } if tool == "claude"));
    let has_strip = plan
        .actions
        .iter()
        .any(|a| matches!(a, RemoveAction::StripMcpEntry { tool, .. } if tool == "claude"));
    assert!(has_shim, "filesystem scan should detect the Claude shim");
    assert!(
        has_strip,
        "filesystem scan should detect mcpServers.synrepo"
    );
}

#[test]
fn per_agent_plan_scoped_to_that_tool_only() {
    let fx = Fixture::new();
    write_claude_shim(fx.path());
    write_mcp_json_with_synrepo(fx.path(), None);

    // A dangling Codex MCP entry the user had set up separately.
    fs::create_dir_all(fx.path().join(".codex")).unwrap();
    fs::write(
        fx.path().join(".codex").join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n",
    )
    .unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    for action in &plan.actions {
        let tool = match action {
            RemoveAction::DeleteShim { tool, .. } | RemoveAction::StripMcpEntry { tool, .. } => {
                tool.as_str()
            }
            other => panic!("per-agent plan should not include {other:?}"),
        };
        assert_eq!(tool, "claude", "per-agent plan leaked into other agents");
    }
}

#[test]
fn apply_strip_codex_mcp_entry_preserves_other_servers() {
    let fx = Fixture::new();
    fs::create_dir_all(fx.path().join(".codex")).unwrap();
    fs::write(
        fx.path().join(".codex").join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n\n[mcp_servers.other]\ncommand = \"other\"\n",
    )
    .unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Codex), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    let raw = fs::read_to_string(fx.path().join(".codex").join("config.toml")).unwrap();
    let v: toml::Value = toml::from_str(&raw).unwrap();
    assert!(
        v.get("mcp_servers")
            .and_then(|servers| servers.get("synrepo"))
            .is_none(),
        "synrepo server entry must be removed"
    );
    assert_eq!(
        v["mcp_servers"]["other"]["command"].as_str().unwrap(),
        "other",
        "other server entry must survive"
    );
}

#[test]
fn apply_strip_mcp_entry_preserves_other_servers() {
    let fx = Fixture::new();
    write_mcp_json_with_synrepo(
        fx.path(),
        Some(("other", json!({ "command": "other-bin", "args": [] }))),
    );

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    let raw = fs::read_to_string(fx.path().join(".mcp.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        v["mcpServers"]["other"].is_object(),
        "other server entry must survive removal"
    );
    assert!(
        v["mcpServers"].get("synrepo").is_none(),
        "synrepo entry must be removed"
    );
}

#[test]
fn apply_strip_mcp_entry_drops_empty_container_but_keeps_file() {
    let fx = Fixture::new();
    write_mcp_json_with_synrepo(fx.path(), None);

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    let raw = fs::read_to_string(fx.path().join(".mcp.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        v.get("mcpServers").is_none(),
        "empty mcpServers should be removed"
    );
    assert!(
        fx.path().join(".mcp.json").exists(),
        "file itself must remain"
    );
}

#[test]
fn apply_delete_shim_cleans_empty_parent_dirs() {
    let fx = Fixture::new();
    write_claude_shim(fx.path());

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    assert!(
        !fx.path().join(".claude").exists(),
        ".claude/ should be removed when the only shim is deleted"
    );
}

#[test]
fn apply_delete_shim_stops_at_non_empty_parent() {
    let fx = Fixture::new();
    write_claude_shim(fx.path());
    // User has another file under .claude/ that is not synrepo-owned.
    fs::write(fx.path().join(".claude").join("keep.md"), b"user file\n").unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    assert!(
        fx.path().join(".claude").join("keep.md").exists(),
        "user's own file must not be touched"
    );
    assert!(
        !fx.path().join(".claude").join("skills").exists(),
        "empty skills/ tree should still be cleaned"
    );
}

#[test]
fn apply_owned_mcp_round_trip_uses_agent_config_uninstall() {
    let fx = Fixture::new();
    let scope = agent_config::Scope::Local(fx.path().to_path_buf());
    let installer = agent_config::mcp_by_id("claude").unwrap();
    let spec = agent_config::McpSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .stdio("synrepo", ["mcp", "--repo", "."])
        .build();
    let _ = installer.install_mcp(&scope, &spec).unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, RemoveAction::StripMcpEntry { tool, .. } if tool == "claude")),
        "owned MCP install should be planned for removal"
    );
    apply_plan(fx.path(), &plan).unwrap();

    let status = installer
        .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
        .unwrap();
    assert!(matches!(status.status, agent_config::InstallStatus::Absent));
}

#[test]
fn apply_owned_skill_round_trip_uses_agent_config_uninstall() {
    let fx = Fixture::new();
    let scope = agent_config::Scope::Local(fx.path().to_path_buf());
    let installer = agent_config::skill_by_id("claude").unwrap();
    let spec = agent_config::SkillSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .description("Use when a repository has synrepo context available.")
        .body("# synrepo\n")
        .build();
    let _ = installer.install_skill(&scope, &spec).unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, RemoveAction::DeleteShim { tool, .. } if tool == "claude")),
        "owned skill install should be planned for removal"
    );
    apply_plan(fx.path(), &plan).unwrap();

    let status = installer
        .skill_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
        .unwrap();
    assert!(matches!(status.status, agent_config::InstallStatus::Absent));
}

#[test]
fn apply_owned_instruction_round_trip_uses_agent_config_uninstall() {
    let fx = Fixture::new();
    let scope = agent_config::Scope::Local(fx.path().to_path_buf());
    let installer = agent_config::instruction_by_id("roo").unwrap();
    let spec = agent_config::InstructionSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .placement(agent_config::InstructionPlacement::StandaloneFile)
        .body("# synrepo\n")
        .build();
    let _ = installer.install_instruction(&scope, &spec).unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Roo), false).unwrap();
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, RemoveAction::DeleteShim { tool, .. } if tool == "roo")),
        "owned instruction install should be planned for removal"
    );
    apply_plan(fx.path(), &plan).unwrap();

    let status = installer
        .instruction_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
        .unwrap();
    assert!(matches!(status.status, agent_config::InstallStatus::Absent));
}

#[test]
fn build_plan_finds_owned_mcp_without_legacy_path() {
    let fx = Fixture::new();
    let scope = agent_config::Scope::Local(fx.path().to_path_buf());
    let installer = agent_config::mcp_by_id("tabnine").unwrap();
    let spec = agent_config::McpSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .stdio("synrepo", ["mcp", "--repo", "."])
        .build();
    let _ = installer.install_mcp(&scope, &spec).unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Tabnine), false).unwrap();
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, RemoveAction::StripMcpEntry { tool, .. } if tool == "tabnine")),
        "agent-config MCP status should drive planning without a hard-coded path"
    );

    apply_plan(fx.path(), &plan).unwrap();
    let status = installer
        .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
        .unwrap();
    assert!(matches!(status.status, agent_config::InstallStatus::Absent));
}

#[test]
fn build_plan_finds_owned_skill_without_legacy_path() {
    let fx = Fixture::new();
    let scope = agent_config::Scope::Local(fx.path().to_path_buf());
    let installer = agent_config::skill_by_id("amp").unwrap();
    let spec = agent_config::SkillSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .description("Use when a repository has synrepo context available.")
        .body("# synrepo\n")
        .build();
    let _ = installer.install_skill(&scope, &spec).unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Amp), false).unwrap();
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, RemoveAction::DeleteShim { tool, .. } if tool == "amp")),
        "agent-config skill status should drive planning without a hard-coded path"
    );

    apply_plan(fx.path(), &plan).unwrap();
    let status = installer
        .skill_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
        .unwrap();
    assert!(matches!(status.status, agent_config::InstallStatus::Absent));
}

#[test]
fn preserved_backup_is_reported_and_not_deleted() {
    let fx = Fixture::new();
    write_mcp_json_with_synrepo(fx.path(), None);
    fs::write(fx.path().join(".mcp.json.bak"), b"{}\n").unwrap();

    let plan = build_plan(fx.path(), None, true).unwrap();
    let bak = fx.path().join(".mcp.json.bak");
    assert!(
        plan.preserved.iter().any(|p| p == &bak),
        "plan must flag the .bak as preserved"
    );

    apply_plan(fx.path(), &plan).unwrap();
    assert!(bak.exists(), ".bak must survive apply");
}

#[test]
fn keep_synrepo_dir_omits_delete_synrepo_action() {
    let fx = Fixture::new();
    fs::create_dir_all(fx.path().join(".synrepo")).unwrap();
    write_claude_shim(fx.path());

    let plan = build_plan(fx.path(), None, true).unwrap();
    let has_delete = plan
        .actions
        .iter()
        .any(|a| matches!(a, RemoveAction::DeleteSynrepoDir));
    assert!(
        !has_delete,
        "keep_synrepo_dir=true must omit DeleteSynrepoDir"
    );
}
