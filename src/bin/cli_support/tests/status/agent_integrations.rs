use tempfile::tempdir;

use super::{seed_graph, status_output};

fn isolated_home() -> (
    synrepo::test_support::GlobalTestLock,
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (lock, home, guard)
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
fn status_json_reports_complete_integration_with_optional_hooks() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let (_lock, _home, _guard) = isolated_home();
    install_codex_skill_and_mcp(repo.path());

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    let rows = json["agent_integrations"].as_array().unwrap();
    let codex = rows
        .iter()
        .find(|row| row["tool"] == "codex")
        .expect("codex integration row");

    assert_eq!(codex["overall"], "complete");
    assert_eq!(codex["context"]["status"], "installed");
    assert_eq!(codex["mcp"]["status"], "installed");
    assert_eq!(codex["hooks"]["status"], "missing");
    assert!(codex["next_action"]
        .as_str()
        .unwrap()
        .contains("--agent-hooks"));
}

#[test]
fn status_json_reports_partial_mcp_only_integration() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let (_lock, _home, _guard) = isolated_home();
    std::fs::write(
        repo.path().join(".mcp.json"),
        r#"{"mcpServers":{"synrepo":{"command":"synrepo","args":["mcp","--repo","."]}}}"#,
    )
    .unwrap();

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    let rows = json["agent_integrations"].as_array().unwrap();
    let claude = rows
        .iter()
        .find(|row| row["tool"] == "claude")
        .expect("claude integration row");

    assert_eq!(claude["overall"], "partial");
    assert_eq!(claude["context"]["status"], "missing");
    assert_eq!(claude["mcp"]["status"], "installed");
    assert_eq!(claude["mcp"]["source"], "legacy config");
}
