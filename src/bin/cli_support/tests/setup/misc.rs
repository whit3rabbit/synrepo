use std::fs;
use std::path::Path;
use tempfile::tempdir;

use agent_config::Scope;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::{step_register_mcp, StepOutcome};

fn setup_mcp(repo_root: &Path, tool: AgentTool, global: bool) -> anyhow::Result<StepOutcome> {
    let scope = if global {
        Scope::Global
    } else {
        Scope::Local(repo_root.to_path_buf())
    };
    step_register_mcp(repo_root, tool, &scope)
}

fn setup_claude_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    setup_mcp(repo_root, AgentTool::Claude, global)
}

fn setup_codex_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    setup_mcp(repo_root, AgentTool::Codex, global)
}

fn setup_opencode_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    setup_mcp(repo_root, AgentTool::OpenCode, global)
}

// ---------- OpenCode: opencode.json ----------

#[test]
fn opencode_preserves_unknown_keys() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("opencode.json");
    fs::write(
        &path,
        r#"{
  "mcp": { "other-server": "other-cmd" },
  "theme": "dark"
}
"#,
    )
    .unwrap();

    setup_opencode_mcp(dir.path(), false).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["theme"], "dark");
    assert_eq!(parsed["mcp"]["other-server"], "other-cmd");
    assert_eq!(parsed["mcp"]["synrepo"]["type"], "local");
    assert_eq!(
        parsed["mcp"]["synrepo"]["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["synrepo", "mcp", "--repo", "."]
    );
}

#[test]
fn opencode_idempotent_on_rerun() {
    let dir = tempdir().unwrap();
    setup_opencode_mcp(dir.path(), false).unwrap();
    let first = fs::read(dir.path().join("opencode.json")).unwrap();
    setup_opencode_mcp(dir.path(), false).unwrap();
    let second = fs::read(dir.path().join("opencode.json")).unwrap();
    assert_eq!(first, second);
}

#[test]
fn opencode_existing_different_unowned_synrepo_is_refused() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("opencode.json");
    fs::write(
        &path,
        r#"{
  "mcp": {
    "synrepo": { "type": "local", "command": ["legacy-bin"] }
  }
}
"#,
    )
    .unwrap();

    let err = setup_opencode_mcp(dir.path(), false).expect_err("unowned synrepo entry must refuse");
    let message = format!("{err:#}");
    assert!(
        message.contains("unowned by agent-config"),
        "error must explain unowned agent-config state: {message}"
    );
    assert!(
        message.contains("opencode.json"),
        "error must name OpenCode config path: {message}"
    );
    assert!(
        message.contains("synrepo setup open-code --project --force"),
        "error must include force recovery command: {message}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcp"]["synrepo"]["command"][0], "legacy-bin");
}

#[test]
fn opencode_global_uses_user_config_without_repo_flag() {
    let _home_lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = crate::cli_support::tests::support::canonicalize_no_verbatim(home.path());
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    let dir = tempdir().unwrap();

    setup_opencode_mcp(dir.path(), true).unwrap();

    let path = canonical_home
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        parsed["mcp"]["synrepo"]["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["synrepo", "mcp"]
    );
    assert!(!dir.path().join("opencode.json").exists());
}

// ---------- Atomic write semantics ----------

#[test]
fn claude_setup_leaves_no_leftover_temp_files() {
    let dir = tempdir().unwrap();
    setup_claude_mcp(dir.path(), false).unwrap();

    for entry in fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains(".tmp."),
            "atomic write left a temp file behind: {name}"
        );
    }
    let final_json = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&final_json).unwrap();
    assert!(parsed["mcpServers"]["synrepo"].is_object());
}

#[test]
fn codex_setup_leaves_no_leftover_temp_files() {
    let dir = tempdir().unwrap();
    setup_codex_mcp(dir.path(), false).unwrap();

    let codex_dir = dir.path().join(".codex");
    for entry in fs::read_dir(&codex_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains(".tmp."),
            "atomic write left a temp file behind: {name}"
        );
    }
    let final_toml = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
    assert!(final_toml.contains("synrepo"));
    let _: toml_edit::DocumentMut = final_toml.parse().unwrap();
}
