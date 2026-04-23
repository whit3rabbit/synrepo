use std::fs;
use tempfile::tempdir;

use crate::cli_support::commands::{
    setup_claude_mcp, setup_cursor_mcp, setup_roo_mcp, setup_windsurf_mcp,
};

// ---------- Claude: .mcp.json ----------

#[test]
fn claude_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mcp.json");
    let original = "{ \"mcpServers\": invalid }";
    fs::write(&path, original).unwrap();

    let err = setup_claude_mcp(dir.path()).expect_err("must error on malformed JSON");
    assert!(
        err.to_string().contains("not valid JSON"),
        "error must name parse failure: {err}"
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

    setup_claude_mcp(dir.path()).unwrap();

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
    setup_claude_mcp(dir.path()).unwrap();
    let first = fs::read(dir.path().join(".mcp.json")).unwrap();
    setup_claude_mcp(dir.path()).unwrap();
    let second = fs::read(dir.path().join(".mcp.json")).unwrap();
    assert_eq!(first, second, "rerun on identical content must be a no-op");
}

#[test]
fn claude_existing_different_synrepo_is_replaced() {
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

    setup_claude_mcp(dir.path()).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
    let args = parsed["mcpServers"]["synrepo"]["args"].as_array().unwrap();
    assert_eq!(args.len(), 3);
}

#[test]
fn claude_rejects_non_object_root() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mcp.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    let err = setup_claude_mcp(dir.path()).expect_err("must error on non-object root");
    assert!(err.to_string().contains("not a JSON object"));
}

// ---------- Cursor: .cursor/mcp.json ----------

#[test]
fn cursor_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();
    fs::write(&path, "{ invalid }").unwrap();

    let err = setup_cursor_mcp(dir.path()).expect_err("must error on malformed JSON");
    assert!(err.to_string().contains("not valid JSON"));
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

    setup_cursor_mcp(dir.path()).unwrap();

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
    setup_cursor_mcp(dir.path()).unwrap();
    let first = fs::read(dir.path().join(".cursor").join("mcp.json")).unwrap();
    setup_cursor_mcp(dir.path()).unwrap();
    let second = fs::read(dir.path().join(".cursor").join("mcp.json")).unwrap();
    assert_eq!(first, second, "rerun on identical content must be a no-op");
}

#[test]
fn cursor_existing_different_synrepo_is_replaced() {
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

    setup_cursor_mcp(dir.path()).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
    let args = parsed["mcpServers"]["synrepo"]["args"].as_array().unwrap();
    assert_eq!(args.len(), 3);
}

#[test]
fn cursor_rejects_non_object_root() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    let err = setup_cursor_mcp(dir.path()).expect_err("must error on non-object root");
    assert!(err.to_string().contains("not a JSON object"));
}

// ---------- Windsurf: .windsurf/mcp.json ----------

#[test]
fn windsurf_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".windsurf").join("mcp.json");
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    fs::write(&path, "{ invalid }").unwrap();

    let err = setup_windsurf_mcp(dir.path()).expect_err("must error on malformed JSON");
    assert!(err.to_string().contains("not valid JSON"));
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
    let path = dir.path().join(".windsurf").join("mcp.json");
    fs::write(
        &path,
        r#"{
  "mcpServers": { "other": { "command": "other-cmd" } },
  "customField": 42
}
"#,
    )
    .unwrap();

    setup_windsurf_mcp(dir.path()).unwrap();

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
    setup_windsurf_mcp(dir.path()).unwrap();
    let first = fs::read(dir.path().join(".windsurf").join("mcp.json")).unwrap();
    setup_windsurf_mcp(dir.path()).unwrap();
    let second = fs::read(dir.path().join(".windsurf").join("mcp.json")).unwrap();
    assert_eq!(first, second, "rerun on identical content must be a no-op");
}

#[test]
fn windsurf_existing_different_synrepo_is_replaced() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    let path = dir.path().join(".windsurf").join("mcp.json");
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

    setup_windsurf_mcp(dir.path()).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
    let args = parsed["mcpServers"]["synrepo"]["args"].as_array().unwrap();
    assert_eq!(args.len(), 3);
}

#[test]
fn windsurf_rejects_non_object_root() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    let path = dir.path().join(".windsurf").join("mcp.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    let err = setup_windsurf_mcp(dir.path()).expect_err("must error on non-object root");
    assert!(err.to_string().contains("not a JSON object"));
}

// ---------- Roo: .roo/mcp.json ----------

#[test]
fn roo_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".roo").join("mcp.json");
    fs::create_dir_all(dir.path().join(".roo")).unwrap();
    fs::write(&path, "{ invalid }").unwrap();

    let err = setup_roo_mcp(dir.path()).expect_err("must error on malformed JSON");
    assert!(err.to_string().contains("not valid JSON"));
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
fn roo_existing_different_synrepo_is_replaced() {
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

    setup_roo_mcp(dir.path()).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["mcpServers"]["synrepo"]["command"], "synrepo");
    let args = parsed["mcpServers"]["synrepo"]["args"].as_array().unwrap();
    assert_eq!(args.len(), 3);
}

#[test]
fn roo_rejects_non_object_root() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".roo")).unwrap();
    let path = dir.path().join(".roo").join("mcp.json");
    fs::write(&path, "[\"not\", \"an\", \"object\"]").unwrap();

    let err = setup_roo_mcp(dir.path()).expect_err("must error on non-object root");
    assert!(err.to_string().contains("not a JSON object"));
}

// opencode and atomic-write tests are in misc.rs.
