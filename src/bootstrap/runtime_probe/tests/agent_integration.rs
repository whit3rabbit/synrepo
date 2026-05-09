use super::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn isolated_home() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    PathBuf,
    crate::config::test_home::HomeEnvGuard,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = home.path().canonicalize().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    (lock, home, canonical_home, guard)
}

#[test]
fn agent_integration_codex_mcp_only_reports_shim_missing() {
    let (_lock, _home, home_path, _guard) = isolated_home();
    let dir = tempdir().unwrap();
    let codex = dir.path().join(".codex");
    fs::create_dir_all(&codex).unwrap();
    fs::write(
        codex.join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n",
    )
    .unwrap();
    fs::write(
        codex.join(".agent-config-mcp.json"),
        r#"{"version":2,"entries":{"synrepo":{"owner":"synrepo","content_hash":"test"}}}"#,
    )
    .unwrap();

    let report = probe_with_home(dir.path(), Some(&home_path));
    assert_eq!(
        report.agent_integration,
        AgentIntegration::McpOnly {
            target: AgentTargetKind::Codex
        }
    );
}

#[test]
fn agent_integration_global_claude_skill_with_project_mcp_is_complete() {
    let (_lock, _home, home_path, _guard) = isolated_home();
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".mcp.json"),
        r#"{"mcpServers":{"synrepo":{"command":"synrepo","args":["mcp","--repo","."]}}}"#,
    )
    .unwrap();
    let skill = agent_config::skill_by_id("claude").unwrap();
    let spec = agent_config::SkillSpec::builder("synrepo")
        .owner("synrepo")
        .description("Use when a repository has synrepo context available.")
        .body("# Synrepo\n")
        .build();
    let _ = skill
        .install_skill(&agent_config::Scope::Global, &spec)
        .unwrap();

    let report = probe_with_home(dir.path(), Some(&home_path));

    assert_eq!(
        report.agent_integration,
        AgentIntegration::Complete {
            target: AgentTargetKind::Claude
        }
    );
}

#[test]
fn agent_integration_prefers_complete_target_over_earlier_mcp_only_hint() {
    let (_lock, _home, home_path, _guard) = isolated_home();
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("CLAUDE.md"), "Claude hint\n").unwrap();
    fs::write(
        dir.path().join(".mcp.json"),
        r#"{"mcpServers":{"synrepo":{"command":"synrepo","args":["mcp","--repo","."]}}}"#,
    )
    .unwrap();
    let codex = dir.path().join(".codex");
    fs::create_dir_all(&codex).unwrap();
    fs::write(
        codex.join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n",
    )
    .unwrap();
    let codex_skill = dir.path().join(".agents").join("skills").join("synrepo");
    fs::create_dir_all(&codex_skill).unwrap();
    fs::write(codex_skill.join("SKILL.md"), b"shim").unwrap();

    let report = probe_with_home(dir.path(), Some(&home_path));

    assert_eq!(
        report.agent_integration,
        AgentIntegration::Complete {
            target: AgentTargetKind::Codex
        }
    );
}
