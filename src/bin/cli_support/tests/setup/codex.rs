use std::fs;
use tempfile::tempdir;

use crate::cli_support::commands::setup_codex_mcp;

#[test]
fn codex_malformed_toml_errors() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    let original = "[mcp\nbroken = ";
    fs::write(&path, original).unwrap();

    let err = setup_codex_mcp(dir.path()).expect_err("must error on malformed TOML");
    assert!(
        err.to_string().contains("not valid TOML"),
        "error must name parse failure: {err}"
    );
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(after, original, "malformed file must not be overwritten");
}

#[test]
fn codex_preserves_comments() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(
        &path,
        "# top-level comment\n[other]\nfoo = \"bar\"\n\n# explains MCP\n[mcp]\n# note: existing setup\nother-server = \"x\"\n",
    )
    .unwrap();

    setup_codex_mcp(dir.path()).unwrap();

    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("# top-level comment"));
    assert!(after.contains("# explains MCP"));
    assert!(after.contains("# note: existing setup"));
    assert!(after.contains("other-server = \"x\""));
    assert!(after.contains("[mcp_servers.synrepo]"));
    assert!(after.contains(r#"command = "synrepo""#));
    assert!(after.contains(r#"args = ["mcp", "--repo", "."]"#));
    assert!(after.contains("[other]"));
    assert!(after.contains("foo = \"bar\""));
}

#[test]
fn codex_idempotent_on_rerun() {
    let dir = tempdir().unwrap();
    setup_codex_mcp(dir.path()).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    let first = fs::read(&path).unwrap();
    setup_codex_mcp(dir.path()).unwrap();
    let second = fs::read(&path).unwrap();
    assert_eq!(
        first, second,
        "rerun on already-current file must be byte-identical"
    );
}

#[test]
fn codex_commented_out_synrepo_adds_active_entry() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(&path, "[mcp]\n# synrepo = \"old-command\"\n").unwrap();

    setup_codex_mcp(dir.path()).unwrap();

    let after = fs::read_to_string(&path).unwrap();
    assert!(
        after.contains("# synrepo = \"old-command\""),
        "commented-out line must survive: {after}"
    );
    assert!(
        after.contains("[mcp_servers.synrepo]"),
        "active synrepo entry must be added: {after}"
    );
}

#[test]
fn codex_duplicate_in_different_section_untouched() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(
        &path,
        "[other]\nsynrepo = \"unrelated\"\n\n[mcp]\nkeep-me = \"yes\"\n",
    )
    .unwrap();

    setup_codex_mcp(dir.path()).unwrap();

    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["other"]["synrepo"].as_str().unwrap(),
        "unrelated",
        "entry under the unrelated section must be untouched"
    );
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["command"]
            .as_str()
            .unwrap(),
        "synrepo",
        "[mcp_servers.synrepo].command must be registered"
    );
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp", "--repo", "."],
        "[mcp_servers.synrepo].args must be registered"
    );
    assert_eq!(
        parsed["mcp"]["keep-me"].as_str().unwrap(),
        "yes",
        "existing sibling in [mcp] must survive"
    );
}

#[test]
fn codex_existing_different_synrepo_is_replaced() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(
        &path,
        "[mcp_servers.synrepo]\ncommand = \"legacy-bin\"\nargs = [\"x\"]\n",
    )
    .unwrap();

    setup_codex_mcp(dir.path()).unwrap();

    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["command"]
            .as_str()
            .unwrap(),
        "synrepo"
    );
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp", "--repo", "."]
    );
}

#[test]
fn codex_legacy_mcp_synrepo_is_migrated() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(&path, "[mcp]\nsynrepo = \"legacy-binary-path\"\n").unwrap();

    setup_codex_mcp(dir.path()).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&raw).unwrap();
    assert!(
        parsed.get("mcp").and_then(|v| v.get("synrepo")).is_none(),
        "legacy [mcp].synrepo must be removed: {raw}"
    );
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["command"]
            .as_str()
            .unwrap(),
        "synrepo"
    );
}

#[test]
fn codex_rejects_non_table_mcp() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(&path, "mcp_servers = \"not a table\"\n").unwrap();

    let err = setup_codex_mcp(dir.path()).expect_err("must error on non-table mcp_servers");
    assert!(err.to_string().contains("not a table"));
}
