//! Tests for `synrepo setup`'s per-client MCP config editors.
//!
//! The helpers under test (`setup_claude_mcp`, `setup_codex_mcp`,
//! `setup_opencode_mcp`) mutate local config files that the user may have
//! hand-edited. The regressions guarded here are the silent-reset bugs from
//! the previous implementations: malformed JSON was replaced with `{}`, TOML
//! was edited via naive string substring matching that clobbered comments or
//! mismatched sections. The tests verify: (1) malformed input errors loudly
//! instead of overwriting; (2) unknown sibling keys and comments survive;
//! (3) re-running is idempotent when the entry is already correct; (4) a
//! pre-existing `synrepo` entry with a different value is replaced.

use std::fs;
use tempfile::tempdir;

use crate::cli_support::commands::{setup_claude_mcp, setup_codex_mcp, setup_opencode_mcp};

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

// ---------- OpenCode: opencode.json ----------

#[test]
fn opencode_malformed_json_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("opencode.json");
    let original = "{ bogus";
    fs::write(&path, original).unwrap();

    let err = setup_opencode_mcp(dir.path()).expect_err("must error on malformed JSON");
    assert!(err.to_string().contains("not valid JSON"));
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(after, original, "malformed file must not be overwritten");
}

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

    setup_opencode_mcp(dir.path()).unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed["theme"], "dark");
    assert_eq!(parsed["mcp"]["other-server"], "other-cmd");
    assert_eq!(parsed["mcp"]["synrepo"], "synrepo mcp --repo .");
}

#[test]
fn opencode_idempotent_on_rerun() {
    let dir = tempdir().unwrap();
    setup_opencode_mcp(dir.path()).unwrap();
    let first = fs::read(dir.path().join("opencode.json")).unwrap();
    setup_opencode_mcp(dir.path()).unwrap();
    let second = fs::read(dir.path().join("opencode.json")).unwrap();
    assert_eq!(first, second);
}

// ---------- Codex: .codex/config.toml ----------

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
    assert!(after.contains(r#"synrepo = "synrepo mcp --repo .""#));
    // [other] table must be untouched.
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
        after.contains(r#"synrepo = "synrepo mcp --repo .""#),
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
        parsed["mcp"]["synrepo"].as_str().unwrap(),
        "synrepo mcp --repo .",
        "[mcp].synrepo must be registered"
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
    fs::write(&path, "[mcp]\nsynrepo = \"legacy-binary-path\"\n").unwrap();

    setup_codex_mcp(dir.path()).unwrap();

    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["mcp"]["synrepo"].as_str().unwrap(),
        "synrepo mcp --repo ."
    );
}

#[test]
fn codex_rejects_non_table_mcp() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(&path, "mcp = \"not a table\"\n").unwrap();

    let err = setup_codex_mcp(dir.path()).expect_err("must error on non-table mcp");
    assert!(err.to_string().contains("not a table"));
}

// ---------- Atomic write semantics ----------
//
// Config edits go through a tempfile + rename so a crash or ENOSPC mid-write
// can never leave a truncated config behind. These tests pin that invariant
// by confirming no leftover temp files remain after a successful run, and
// that the rename lands an intact, parseable result.

#[test]
fn claude_setup_leaves_no_leftover_temp_files() {
    let dir = tempdir().unwrap();
    setup_claude_mcp(dir.path()).unwrap();

    // The atomic writer names temps `.<filename>.tmp.<pid>.<nanos>`.
    for entry in fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains(".tmp."),
            "atomic write left a temp file behind: {name}"
        );
    }
    let final_json = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
    // Must be parseable JSON with the expected entry, not a truncated blob.
    let parsed: serde_json::Value = serde_json::from_str(&final_json).unwrap();
    assert!(parsed["mcpServers"]["synrepo"].is_object());
}

#[test]
fn codex_setup_leaves_no_leftover_temp_files() {
    let dir = tempdir().unwrap();
    setup_codex_mcp(dir.path()).unwrap();

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
    // Parsing must succeed — a truncated write would produce invalid TOML.
    let _: toml_edit::DocumentMut = final_toml.parse().unwrap();
}
