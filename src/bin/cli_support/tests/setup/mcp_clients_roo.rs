use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::{step_register_mcp, StepOutcome};

fn setup_roo_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome> {
    let scope = agent_config::Scope::Local(repo_root.to_path_buf());
    step_register_mcp(repo_root, AgentTool::Roo, &scope)
}

#[test]
fn roo_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".roo").join("mcp.json");
    fs::create_dir_all(dir.path().join(".roo")).unwrap();
    fs::write(&path, "{ invalid }").unwrap();

    let err = setup_roo_mcp(dir.path()).expect_err("must error on malformed JSON");
    assert!(format!("{err:#}").contains("invalid JSON"));
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, "{ invalid }",
        "malformed file must not be overwritten"
    );
}

#[test]
fn roo_preserves_unknown_keys() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".roo")).unwrap();
    let path = dir.path().join(".roo").join("mcp.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": { "other": { "command": "other-cmd" } },
  "customField": 42
}
"#,
    )
    .unwrap();

    setup_roo_mcp(dir.path()).unwrap();

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
fn roo_duplicate_registration_idempotent() {
    let dir = tempdir().unwrap();
    setup_roo_mcp(dir.path()).unwrap();
    let first = fs::read(dir.path().join(".roo").join("mcp.json")).unwrap();
    setup_roo_mcp(dir.path()).unwrap();
    let second = fs::read(dir.path().join(".roo").join("mcp.json")).unwrap();
    assert_eq!(first, second, "rerun on identical content must be a no-op");
}

#[test]
fn roo_existing_different_unowned_synrepo_is_refused() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".roo")).unwrap();
    let path = dir.path().join(".roo").join("mcp.json");
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

    let err = setup_roo_mcp(dir.path()).expect_err("unowned synrepo entry must refuse");
    assert!(
        format!("{err:#}").contains("not owned by caller"),
        "unexpected error: {err:#}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "legacy-bin");
}

#[test]
fn roo_replaces_non_object_root_via_installer() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".roo")).unwrap();
    let path = dir.path().join(".roo").join("mcp.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    setup_roo_mcp(dir.path()).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
}
