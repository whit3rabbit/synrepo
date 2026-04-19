//! Round-trip tests for the registry. These drive the `io::*` helpers
//! directly so the tests never touch a real `$HOME`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::{io, AgentEntry, ProjectEntry, Registry, SCHEMA_VERSION};

fn sample_project(path: &Path) -> ProjectEntry {
    ProjectEntry {
        path: path.to_path_buf(),
        initialized_at: "2026-04-19T00:00:00Z".to_string(),
        synrepo_dir: ".synrepo".to_string(),
        root_gitignore_entry_added: true,
        export_gitignore_entry_added: false,
        agents: vec![AgentEntry {
            tool: "claude".to_string(),
            shim_path: ".claude/skills/synrepo/SKILL.md".to_string(),
            mcp_config_path: Some(".mcp.json".to_string()),
            mcp_backup_path: Some(".mcp.json.bak".to_string()),
            installed_at: "2026-04-19T00:00:05Z".to_string(),
        }],
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
        path: dir.path().to_path_buf(),
        initialized_at: "2026-04-19T00:00:00Z".to_string(),
        synrepo_dir: ".synrepo".to_string(),
        root_gitignore_entry_added: false,
        export_gitignore_entry_added: false,
        agents: vec![AgentEntry {
            tool: "copilot".to_string(),
            shim_path: "synrepo-copilot-instructions.md".to_string(),
            mcp_config_path: None,
            mcp_backup_path: None,
            installed_at: "2026-04-19T00:00:00Z".to_string(),
        }],
    });
    io::save_to(&path, &registry).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("mcp_config_path"),
        "None fields should be skipped: {text}"
    );
    assert!(!text.contains("mcp_backup_path"));
}
