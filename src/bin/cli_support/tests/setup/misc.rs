use std::fs;
use tempfile::tempdir;

use crate::cli_support::commands::{setup_claude_mcp, setup_codex_mcp, setup_opencode_mcp};

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
    assert_eq!(parsed["mcp"]["synrepo"], "synrepo mcp --repo .");
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
