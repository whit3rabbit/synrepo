use std::fs;
use std::path::Path;
use tempfile::tempdir;

use agent_config::{InstallStatus, Scope};

use crate::cli_support::agent_shims::{AgentTool, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::cli_support::commands::{step_register_mcp, StepOutcome};

fn setup_mcp(repo_root: &Path, tool: AgentTool) -> anyhow::Result<StepOutcome> {
    let scope = Scope::Local(repo_root.to_path_buf());
    step_register_mcp(repo_root, tool, &scope)
}

fn setup_claude_mcp(repo_root: &Path, _global: bool) -> anyhow::Result<StepOutcome> {
    setup_mcp(repo_root, AgentTool::Claude)
}

fn setup_cursor_mcp(repo_root: &Path, _global: bool) -> anyhow::Result<StepOutcome> {
    setup_mcp(repo_root, AgentTool::Cursor)
}

fn setup_windsurf_mcp(repo_root: &Path, _global: bool) -> anyhow::Result<StepOutcome> {
    setup_mcp(repo_root, AgentTool::Windsurf)
}

fn assert_unowned_refusal_guidance(err: &anyhow::Error, tool: AgentTool, path: &str) {
    let message = format!("{err:#}");
    let normalized = message.replace('\\', "/");
    assert!(
        message.contains("unowned by agent-config"),
        "error must explain unowned agent-config state: {message}"
    );
    assert!(
        normalized.contains(path),
        "error must name MCP config path {path}: {message}"
    );
    assert!(
        message.contains(&format!(
            "synrepo setup {} --project --force",
            tool.canonical_name()
        )),
        "error must include force recovery command: {message}"
    );
}

fn assert_identical_unowned_synrepo_is_adopted(tool: AgentTool) {
    let dir = tempdir().unwrap();
    let scope = Scope::Local(dir.path().to_path_buf());
    setup_mcp(dir.path(), tool).unwrap();

    let installer = agent_config::mcp_by_id(tool.agent_config_id().unwrap()).unwrap();
    let status = installer
        .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
        .unwrap();
    let config_path = status.config_path.unwrap();
    fs::remove_file(status.ledger_path.unwrap()).unwrap();

    let before = fs::read(&config_path).unwrap();
    let outcome = setup_mcp(dir.path(), tool).unwrap();
    assert_eq!(outcome, StepOutcome::AlreadyCurrent);
    assert_eq!(
        fs::read(&config_path).unwrap(),
        before,
        "ledger-only adoption must not rewrite MCP config"
    );
    assert!(matches!(
        installer
            .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .unwrap()
            .status,
        InstallStatus::InstalledOwned { ref owner } if owner == SYNREPO_INSTALL_OWNER
    ));

    let after_adoption = fs::read(&config_path).unwrap();
    setup_mcp(dir.path(), tool).unwrap();
    assert_eq!(
        fs::read(&config_path).unwrap(),
        after_adoption,
        "rerun after adoption must be byte-identical"
    );
}

// ---------- Claude: .mcp.json ----------

#[test]
fn claude_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mcp.json");
    let original = "{ \"mcpServers\": invalid }";
    fs::write(&path, original).unwrap();

    let err = setup_claude_mcp(dir.path(), false).expect_err("must error on malformed JSON");
    let message = format!("{err:#}");
    assert!(
        message.contains("invalid JSON"),
        "error must name parse failure: {message}"
    );
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(after, original, "malformed file must not be overwritten");
}

#[test]
fn claude_preserves_unknown_keys() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mcp.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": { "other": { "command": "other-cmd" } },
  "customField": 42
}
"#,
    )
    .unwrap();

    setup_claude_mcp(dir.path(), false).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["customField"], 42,
        "top-level unknown key must survive"
    );
    assert_eq!(
        parsed["mcpServers"]["other"]["command"], "other-cmd",
        "unrelated server entry must survive"
    );
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}

#[test]
fn claude_duplicate_registration_idempotent() {
    let dir = tempdir().unwrap();
    setup_claude_mcp(dir.path(), false).unwrap();
    let first = fs::read(dir.path().join(".mcp.json")).unwrap();
    setup_claude_mcp(dir.path(), false).unwrap();
    let second = fs::read(dir.path().join(".mcp.json")).unwrap();
    assert_eq!(first, second, "rerun on identical content must be a no-op");
}

#[test]
fn claude_existing_different_unowned_synrepo_is_refused() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mcp.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": {
    "synrepo": { "command": "legacy-bin", "args": ["x"] }
  }
}
"#,
    )
    .unwrap();

    let err = setup_claude_mcp(dir.path(), false).expect_err("unowned synrepo entry must refuse");
    assert_unowned_refusal_guidance(&err, AgentTool::Claude, ".mcp.json");

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "legacy-bin");
}

#[test]
fn claude_identical_unowned_synrepo_is_adopted_without_rewrite() {
    assert_identical_unowned_synrepo_is_adopted(AgentTool::Claude);
}

#[test]
fn claude_replaces_non_object_root_via_installer() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mcp.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    setup_claude_mcp(dir.path(), false).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}

// ---------- Cursor: .cursor/mcp.json ----------

#[test]
fn cursor_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();
    fs::write(&path, "{ invalid }").unwrap();

    let err = setup_cursor_mcp(dir.path(), false).expect_err("must error on malformed JSON");
    assert!(format!("{err:#}").contains("invalid JSON"));
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, "{ invalid }",
        "malformed file must not be overwritten"
    );
}

#[test]
fn cursor_preserves_unknown_keys() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": { "other": { "command": "other-cmd" } },
  "customField": 42
}
"#,
    )
    .unwrap();

    setup_cursor_mcp(dir.path(), false).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["customField"], 42,
        "top-level unknown key must survive"
    );
    assert_eq!(
        parsed["mcpServers"]["other"]["command"], "other-cmd",
        "unrelated server entry must survive"
    );
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}

#[test]
fn cursor_duplicate_registration_idempotent() {
    let dir = tempdir().unwrap();
    setup_cursor_mcp(dir.path(), false).unwrap();
    let first = fs::read(dir.path().join(".cursor").join("mcp.json")).unwrap();
    setup_cursor_mcp(dir.path(), false).unwrap();
    let second = fs::read(dir.path().join(".cursor").join("mcp.json")).unwrap();
    assert_eq!(first, second, "rerun on identical content must be a no-op");
}

#[test]
fn cursor_existing_different_unowned_synrepo_is_refused() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": {
    "synrepo": { "command": "legacy-bin", "args": ["x"] }
  }
}
"#,
    )
    .unwrap();

    let err = setup_cursor_mcp(dir.path(), false).expect_err("unowned synrepo entry must refuse");
    assert_unowned_refusal_guidance(&err, AgentTool::Cursor, ".cursor/mcp.json");

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "legacy-bin");
}

#[test]
fn cursor_replaces_non_object_root_via_installer() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    setup_cursor_mcp(dir.path(), false).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}

// ---------- Windsurf: .windsurf/mcp.json ----------

#[test]
fn windsurf_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".windsurf").join("mcp_config.json");
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    fs::write(&path, "{ invalid }").unwrap();

    let err = setup_windsurf_mcp(dir.path(), false).expect_err("must error on malformed JSON");
    assert!(format!("{err:#}").contains("invalid JSON"));
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, "{ invalid }",
        "malformed file must not be overwritten"
    );
}

#[test]
fn windsurf_preserves_unknown_keys() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    let path = dir.path().join(".windsurf").join("mcp_config.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": { "other": { "command": "other-cmd" } },
  "customField": 42
}
"#,
    )
    .unwrap();

    setup_windsurf_mcp(dir.path(), false).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["customField"], 42,
        "top-level unknown key must survive"
    );
    assert_eq!(
        parsed["mcpServers"]["other"]["command"], "other-cmd",
        "unrelated server entry must survive"
    );
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}

#[test]
fn windsurf_duplicate_registration_idempotent() {
    let dir = tempdir().unwrap();
    setup_windsurf_mcp(dir.path(), false).unwrap();
    let first = fs::read(dir.path().join(".windsurf").join("mcp_config.json")).unwrap();
    setup_windsurf_mcp(dir.path(), false).unwrap();
    let second = fs::read(dir.path().join(".windsurf").join("mcp_config.json")).unwrap();
    assert_eq!(first, second, "rerun on identical content must be a no-op");
}

#[test]
fn windsurf_existing_different_unowned_synrepo_is_refused() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    let path = dir.path().join(".windsurf").join("mcp_config.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": {
    "synrepo": { "command": "legacy-bin", "args": ["x"] }
  }
}
"#,
    )
    .unwrap();

    let err = setup_windsurf_mcp(dir.path(), false).expect_err("unowned synrepo entry must refuse");
    assert_unowned_refusal_guidance(&err, AgentTool::Windsurf, ".windsurf/mcp_config.json");

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "legacy-bin");
}

#[test]
fn windsurf_replaces_non_object_root_via_installer() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    let path = dir.path().join(".windsurf").join("mcp_config.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    setup_windsurf_mcp(dir.path(), false).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}

// Roo, opencode, and atomic-write tests are in sibling setup modules.
