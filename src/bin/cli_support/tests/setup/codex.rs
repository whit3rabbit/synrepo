use std::ffi::OsString;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

use agent_config::Scope;
use synrepo::bootstrap::runtime_probe::AgentTargetKind;
use synrepo::tui::McpInstallPlan;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::{step_register_mcp, StepOutcome};
use crate::cli_support::repair_cmd::execute_project_mcp_install_plan;

fn setup_codex_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    let scope = if global {
        Scope::Global
    } else {
        Scope::Local(repo_root.to_path_buf())
    };
    step_register_mcp(repo_root, AgentTool::Codex, &scope)
}

struct EnvGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &Path) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
fn codex_malformed_toml_errors() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    let original = "[mcp\nbroken = ";
    fs::write(&path, original).unwrap();

    let err = setup_codex_mcp(dir.path(), false).expect_err("must error on malformed TOML");
    let message = format!("{err:#}");
    assert!(
        message.contains("invalid TOML"),
        "error must name parse failure: {message}"
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

    setup_codex_mcp(dir.path(), false).unwrap();

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
    setup_codex_mcp(dir.path(), false).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    let first = fs::read(&path).unwrap();
    setup_codex_mcp(dir.path(), false).unwrap();
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

    setup_codex_mcp(dir.path(), false).unwrap();

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

    setup_codex_mcp(dir.path(), false).unwrap();

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
fn codex_existing_different_unowned_synrepo_is_refused() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(
        &path,
        "[mcp_servers.synrepo]\ncommand = \"legacy-bin\"\nargs = [\"x\"]\n",
    )
    .unwrap();

    let err = setup_codex_mcp(dir.path(), false).expect_err("unowned synrepo entry must refuse");
    assert!(
        format!("{err:#}").contains("not owned by caller"),
        "unexpected error: {err:#}"
    );

    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["command"]
            .as_str()
            .unwrap(),
        "legacy-bin",
        "setup must not take over unowned legacy content"
    );
}

#[test]
fn codex_legacy_mcp_synrepo_is_left_for_upgrade_migration() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    fs::write(&path, "[mcp]\nsynrepo = \"legacy-binary-path\"\n").unwrap();

    setup_codex_mcp(dir.path(), false).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&raw).unwrap();
    assert!(
        parsed.get("mcp").and_then(|v| v.get("synrepo")).is_some(),
        "legacy [mcp].synrepo must be left for upgrade adoption: {raw}"
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
    let original = "mcp_servers = \"not a table\"\n";
    fs::write(&path, original).unwrap();

    let err = setup_codex_mcp(dir.path(), false).expect_err("must error on non-table mcp_servers");
    assert!(
        format!("{err:#}").contains("failed to register synrepo MCP for Codex CLI"),
        "unexpected error: {err:#}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn codex_global_uses_codex_home_without_repo_flag() {
    let home = tempdir().unwrap();
    let codex_home = crate::cli_support::tests::support::canonicalize_no_verbatim(home.path());
    let _lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let _guard = EnvGuard::set("CODEX_HOME", &codex_home);
    let dir = tempdir().unwrap();

    setup_codex_mcp(dir.path(), true).unwrap();

    let parsed: toml::Value =
        toml::from_str(&fs::read_to_string(codex_home.join("config.toml")).unwrap()).unwrap();
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp"]
    );
    assert!(!dir.path().join(".codex").exists());
}

#[test]
fn codex_dashboard_mcp_install_is_repo_local_mcp_only() {
    let home = tempdir().unwrap();
    let codex_home = crate::cli_support::tests::support::canonicalize_no_verbatim(home.path());
    let _lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let _guard = EnvGuard::set("CODEX_HOME", &codex_home);
    fs::create_dir_all(&codex_home).unwrap();
    let global_config = codex_home.join("config.toml");
    let global_before = "[profiles.default]\nmodel = \"gpt-test\"\n";
    fs::write(&global_config, global_before).unwrap();
    let repo = tempdir().unwrap();

    execute_project_mcp_install_plan(
        repo.path(),
        McpInstallPlan {
            target: AgentTargetKind::Codex,
        },
    )
    .unwrap();

    let local_config = repo.path().join(".codex").join("config.toml");
    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&local_config).unwrap()).unwrap();
    assert_eq!(
        parsed["mcp_servers"]["synrepo"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp", "--repo", "."],
        "dashboard MCP tab install must use repo-local command args"
    );
    assert!(
        !repo
            .path()
            .join(".agents")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md")
            .exists(),
        "MCP-only install must not write the Codex skill"
    );
    assert_eq!(
        fs::read_to_string(&global_config).unwrap(),
        global_before,
        "project MCP install must not touch global Codex config"
    );
}
