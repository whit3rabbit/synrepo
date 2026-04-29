//! Round-trip tests for the registry. These drive the `io::*` helpers
//! directly so the tests never touch a real `$HOME`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::{io, AgentEntry, HookEntry, ProjectEntry, Registry, SCHEMA_VERSION};

fn sample_project(path: &Path) -> ProjectEntry {
    ProjectEntry {
        id: String::new(),
        path: path.to_path_buf(),
        name: None,
        last_opened_at: None,
        initialized_at: "2026-04-19T00:00:00Z".to_string(),
        synrepo_dir: ".synrepo".to_string(),
        root_gitignore_entry_added: true,
        export_gitignore_entry_added: false,
        agents: vec![AgentEntry {
            tool: "claude".to_string(),
            scope: "project".to_string(),
            shim_path: ".claude/skills/synrepo/SKILL.md".to_string(),
            mcp_config_path: Some(".mcp.json".to_string()),
            mcp_backup_path: Some(".mcp.json.bak".to_string()),
            installed_at: "2026-04-19T00:00:05Z".to_string(),
        }],
        hooks: Vec::new(),
    }
}

#[test]
fn load_from_missing_file_returns_empty_registry() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    let registry = io::load_from(&path).unwrap();
    assert!(registry.projects.is_empty());
}

#[test]
fn load_from_empty_file_returns_empty_registry() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    fs::write(&path, "").unwrap();
    let registry = io::load_from(&path).unwrap();
    assert!(registry.projects.is_empty());
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    let mut registry = Registry::default();
    registry.projects.push(sample_project(dir.path()));

    io::save_to(&path, &registry).unwrap();
    let reloaded = io::load_from(&path).unwrap();

    assert_eq!(reloaded.schema_version, SCHEMA_VERSION);
    assert_eq!(reloaded.projects.len(), 1);
    let p = &reloaded.projects[0];
    assert_eq!(p.path, PathBuf::from(dir.path()));
    assert!(p.root_gitignore_entry_added);
    assert_eq!(p.agents.len(), 1);
    assert_eq!(p.agents[0].tool, "claude");
    assert_eq!(
        p.agents[0].mcp_backup_path.as_deref(),
        Some(".mcp.json.bak")
    );
}

#[test]
fn save_stamps_current_schema_version_even_if_registry_value_is_stale() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    let mut registry = Registry::default();
    registry.schema_version = 0;
    registry.projects.push(sample_project(dir.path()));
    io::save_to(&path, &registry).unwrap();

    let reloaded = io::load_from(&path).unwrap();
    assert_eq!(reloaded.schema_version, SCHEMA_VERSION);
}

#[test]
fn save_creates_parent_directory() {
    let dir = tempdir().unwrap();
    let path = dir
        .path()
        .join("nested")
        .join("deeper")
        .join("projects.toml");
    let registry = Registry::default();
    io::save_to(&path, &registry).unwrap();
    assert!(path.exists());
}

#[test]
fn malformed_toml_surfaces_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    fs::write(&path, "this is not toml = @@@").unwrap();
    let err = io::load_from(&path).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("failed to parse registry"),
        "unexpected error: {msg}"
    );
}

#[test]
fn default_registry_load_has_empty_projects_and_zero_version() {
    let registry = Registry::default();
    assert_eq!(registry.schema_version, 0);
    assert!(registry.projects.is_empty());
}

#[test]
fn agent_entry_omits_optional_fields_when_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    let mut registry = Registry::default();
    registry.projects.push(ProjectEntry {
        id: String::new(),
        path: dir.path().to_path_buf(),
        name: None,
        last_opened_at: None,
        initialized_at: "2026-04-19T00:00:00Z".to_string(),
        synrepo_dir: ".synrepo".to_string(),
        root_gitignore_entry_added: false,
        export_gitignore_entry_added: false,
        agents: vec![AgentEntry {
            tool: "copilot".to_string(),
            scope: "project".to_string(),
            shim_path: "synrepo-copilot-instructions.md".to_string(),
            mcp_config_path: None,
            mcp_backup_path: None,
            installed_at: "2026-04-19T00:00:00Z".to_string(),
        }],
        hooks: Vec::new(),
    });
    io::save_to(&path, &registry).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("mcp_config_path"),
        "None fields should be skipped: {text}"
    );
    assert!(!text.contains("mcp_backup_path"));
}

#[test]
fn hook_entries_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    let mut project = sample_project(dir.path());
    project.hooks.push(HookEntry {
        name: "post-commit".to_string(),
        path: ".git/hooks/post-commit".to_string(),
        mode: "marked_block".to_string(),
        installed_at: "2026-04-29T00:00:00Z".to_string(),
    });
    let mut registry = Registry::default();
    registry.projects.push(project);

    io::save_to(&path, &registry).unwrap();
    let reloaded = io::load_from(&path).unwrap();
    assert_eq!(reloaded.projects[0].hooks.len(), 1);
    assert_eq!(reloaded.projects[0].hooks[0].name, "post-commit");
}

#[test]
fn record_project_preserves_existing_metadata() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let project = tempdir().unwrap();
    let agent = AgentEntry {
        tool: "claude".to_string(),
        scope: "project".to_string(),
        shim_path: ".claude/skills/synrepo/SKILL.md".to_string(),
        mcp_config_path: Some(".mcp.json".to_string()),
        mcp_backup_path: None,
        installed_at: "2026-04-19T00:00:05Z".to_string(),
    };
    super::record_agent(project.path(), agent.clone()).unwrap();
    let before = super::get(project.path()).unwrap().unwrap();

    let after = super::record_project(project.path()).unwrap();

    assert_eq!(after.initialized_at, before.initialized_at);
    assert_eq!(after.agents, vec![agent]);
}

#[test]
fn load_project_entry_with_missing_defaulted_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    fs::write(
        &path,
        format!(
            "schema_version = 1\n\n[[project]]\npath = \"{}\"\ninitialized_at = \"2026-04-19T00:00:00Z\"\n",
            dir.path().display()
        ),
    )
    .unwrap();

    let registry = io::load_from(&path).unwrap();
    let project = &registry.projects[0];
    assert_eq!(project.synrepo_dir, ".synrepo");
    assert!(project.id.is_empty());
    assert!(project.effective_id().starts_with("proj_"));
    assert_eq!(
        project.display_name(),
        super::default_project_name(dir.path())
    );
    assert!(!project.root_gitignore_entry_added);
    assert!(project.agents.is_empty());
}

#[test]
fn record_project_sets_id_and_default_display_name() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let project = tempdir().unwrap();

    let entry = super::record_project(project.path()).unwrap();

    assert!(entry.id.starts_with("proj_"));
    assert_eq!(
        entry.display_name(),
        project.path().file_name().unwrap().to_string_lossy()
    );
    assert_eq!(entry.effective_id(), entry.id);
}

#[test]
fn rename_project_preserves_identity_path_and_install_metadata() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let project = tempdir().unwrap();
    let agent = AgentEntry {
        tool: "codex".to_string(),
        scope: "global".to_string(),
        shim_path: "/tmp/synrepo-skill.md".to_string(),
        mcp_config_path: Some("/tmp/config.toml".to_string()),
        mcp_backup_path: None,
        installed_at: "2026-04-19T00:00:05Z".to_string(),
    };
    super::record_agent(project.path(), agent.clone()).unwrap();
    let before = super::record_project(project.path()).unwrap();

    let renamed = super::rename_project(&before.id, "agent-config").unwrap();

    assert_eq!(renamed.id, before.id);
    assert_eq!(renamed.path, before.path);
    assert_eq!(renamed.initialized_at, before.initialized_at);
    assert_eq!(renamed.agents, vec![agent]);
    assert_eq!(renamed.name.as_deref(), Some("agent-config"));
}

#[test]
fn duplicate_display_names_require_id_or_path_selection() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let root = tempdir().unwrap();
    let left = root.path().join("left").join("synrepo");
    let right = root.path().join("right").join("synrepo");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    let first = super::record_project(&left).unwrap();
    let second = super::record_project(&right).unwrap();

    let err = super::resolve_project("synrepo").unwrap_err();
    let msg = format!("{err:#}");

    assert!(msg.contains("multiple projects match"), "{msg}");
    assert!(msg.contains(&first.id), "{msg}");
    assert!(msg.contains(&second.id), "{msg}");
}

#[test]
fn mark_project_opened_updates_last_opened() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let project = tempdir().unwrap();
    let entry = super::record_project(project.path()).unwrap();

    let opened = super::mark_project_opened(&entry.id).unwrap();

    assert_eq!(opened.id, entry.id);
    assert!(opened.last_opened_at.is_some());
}
