use super::super::finalize_remove;
use super::*;

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
fn build_plan_removes_syntext_gitignore_only_when_registry_owns_it() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = TempDir::new().unwrap();
    fs::write(repo.path().join(".gitignore"), ".syntext\n").unwrap();

    let untracked = build_plan(repo.path(), None, false).unwrap();
    assert!(!untracked.actions.iter().any(|action| matches!(
        action,
        RemoveAction::RemoveGitignoreLine { entry } if entry == ".syntext/"
    )));

    synrepo::registry::record_syntext_gitignore(repo.path(), true).unwrap();
    let tracked = build_plan(repo.path(), None, false).unwrap();
    assert!(!tracked.actions.iter().any(|action| matches!(
        action,
        RemoveAction::RemoveGitignoreLine { entry } if entry == ".syntext/"
    )));

    fs::write(repo.path().join(".gitignore"), ".syntext/\n").unwrap();
    let tracked_exact = build_plan(repo.path(), None, false).unwrap();
    assert!(tracked_exact.actions.iter().any(|action| matches!(
        action,
        RemoveAction::RemoveGitignoreLine { entry } if entry == ".syntext/"
    )));
}

#[test]
fn apply_plan_clears_syntext_gitignore_registry_flag() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = TempDir::new().unwrap();
    fs::write(
        repo.path().join(".gitignore"),
        "target/\n.syntext/\n!.gitkeep\n",
    )
    .unwrap();
    synrepo::registry::record_syntext_gitignore(repo.path(), true).unwrap();

    let plan = build_plan(repo.path(), None, false).unwrap();
    finalize_remove(repo.path(), None, &plan, false).unwrap();

    let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert_eq!(gitignore, "target/\n!.gitkeep\n");
    assert!(!gitignore.lines().any(|line| line.trim() == ".syntext/"));
    let entry = synrepo::registry::get(repo.path()).unwrap().unwrap();
    assert!(!entry.syntext_gitignore_entry_added);
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
