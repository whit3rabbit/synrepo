use std::fs;

use tempfile::tempdir;

use agent_config::Scope;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::step_register_mcp;

fn redirect_home() -> (
    synrepo::test_support::GlobalTestLock,
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = home.path().canonicalize().unwrap();
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    (lock, home, guard)
}

fn claude_global_config_path(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".claude.json")
}

#[test]
fn claude_global_uses_user_config_without_repo_flag() {
    let (_lock, home, _guard) = redirect_home();
    let repo = tempdir().unwrap();

    step_register_mcp(repo.path(), AgentTool::Claude, &Scope::Global).unwrap();

    let path = claude_global_config_path(&home.path().canonicalize().unwrap());
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
    assert_eq!(
        parsed["mcpServers"]["synrepo"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp"]
    );
    assert!(!repo.path().join(".mcp.json").exists());
}

#[test]
fn cursor_global_uses_user_config_without_repo_flag() {
    let (_lock, home, _guard) = redirect_home();
    let repo = tempdir().unwrap();

    step_register_mcp(repo.path(), AgentTool::Cursor, &Scope::Global).unwrap();

    let path = home
        .path()
        .canonicalize()
        .unwrap()
        .join(".cursor")
        .join("mcp.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["mcpServers"]["synrepo"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp"]
    );
    assert!(!repo.path().join(".cursor").exists());
}
