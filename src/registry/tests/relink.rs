use std::fs;

use tempfile::tempdir;

use super::{io, sample_project, Registry};

fn isolated_home() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (lock, home, guard)
}

#[test]
fn relink_preserves_identity_and_install_records() {
    let (_lock, home, _guard) = isolated_home();
    let old = home.path().join("old");
    let new = home.path().join("new");
    fs::create_dir_all(old.join(".synrepo")).unwrap();
    fs::write(old.join(".synrepo/config.toml"), "mode = 'auto'\n").unwrap();
    let original = crate::registry::record_project(&old).unwrap();
    crate::registry::rename_project(&original.id, "saved alias").unwrap();
    let before = crate::registry::resolve_project(&original.id).unwrap();
    fs::rename(&old, &new).unwrap();

    assert_eq!(
        crate::registry::relocation_candidates(&before),
        vec![new.canonicalize().unwrap()]
    );
    let result = crate::registry::relink_project(&original.id, &new).unwrap();
    assert_eq!(result.entry.id, original.id);
    assert_eq!(result.entry.name.as_deref(), Some("saved alias"));
    assert_eq!(result.entry.initialized_at, original.initialized_at);
    assert_eq!(result.entry.path, new.canonicalize().unwrap());
    assert!(crate::registry::get(&old).unwrap().is_none());
    assert_eq!(
        crate::registry::resolve_project(&original.id).unwrap().path,
        result.entry.path
    );
}

#[test]
fn init_record_persists_identity_for_rename_hints() {
    let (_lock, home, _guard) = isolated_home();
    let old = home.path().join("old");
    let new = home.path().join("new");
    fs::create_dir_all(old.join(".synrepo")).unwrap();
    fs::write(old.join(".synrepo/config.toml"), "mode = 'auto'\n").unwrap();
    crate::registry::record_install(&old, false).unwrap();
    let entry = crate::registry::get(&old).unwrap().unwrap();
    assert!(old.join(".synrepo/state/project-identity.json").exists());
    fs::rename(&old, &new).unwrap();
    assert_eq!(
        crate::registry::relocation_candidates(&entry),
        vec![new.canonicalize().unwrap()]
    );
}

#[test]
fn relink_rejects_a_managed_destination() {
    let (_lock, home, _guard) = isolated_home();
    let old = home.path().join("old");
    let new = home.path().join("new");
    for path in [&old, &new] {
        fs::create_dir_all(path.join(".synrepo")).unwrap();
        fs::write(path.join(".synrepo/config.toml"), "mode = 'auto'\n").unwrap();
    }
    let first = crate::registry::record_project(&old).unwrap();
    crate::registry::record_project(&new).unwrap();
    let error = crate::registry::relink_project(&first.id, &new)
        .unwrap_err()
        .to_string();
    assert!(error.contains("already managed"), "{error}");
    assert_eq!(
        crate::registry::resolve_project(&first.id).unwrap().path,
        first.path
    );
}

#[test]
fn relink_repairs_owned_mcp_path_reference() {
    let (_lock, home, _guard) = isolated_home();
    let old = home.path().join("old");
    let new = home.path().join("new");
    fs::create_dir_all(old.join(".synrepo")).unwrap();
    fs::write(old.join(".synrepo/config.toml"), "mode = 'auto'\n").unwrap();
    let entry = crate::registry::record_project(&old).unwrap();
    let installer = agent_config::mcp_by_id("claude").unwrap();
    let scope = agent_config::Scope::Local(old.clone());
    let spec = agent_config::McpSpec::builder("synrepo")
        .owner("synrepo")
        .stdio("synrepo", ["mcp", "--repo", entry.path.to_str().unwrap()])
        .build();
    let _ = installer.install_mcp(&scope, &spec).unwrap();
    crate::registry::record_agent(
        &old,
        crate::registry::AgentEntry {
            tool: "claude".to_string(),
            scope: "project".to_string(),
            shim_path: "AGENTS.md".to_string(),
            mcp_config_path: Some(".mcp.json".to_string()),
            mcp_backup_path: None,
            installed_at: entry.initialized_at.clone(),
        },
    )
    .unwrap();
    fs::rename(&old, &new).unwrap();

    let result = crate::registry::relink_project(&entry.id, &new).unwrap();
    assert_eq!(result.repaired_integrations, 1, "{result:?}");
    assert!(result.manual_repairs.is_empty());
    let config = fs::read_to_string(new.join(".mcp.json")).unwrap();
    assert!(config.contains(new.to_str().unwrap()));
    assert!(!config.contains(entry.path.to_str().unwrap()));
}

#[test]
fn prune_thousands_of_missing_rows_keeps_live_entry() {
    let (_lock, home, _guard) = isolated_home();
    let live = home.path().join("live");
    fs::create_dir(&live).unwrap();
    let mut registry = Registry::default();
    registry.projects.push(sample_project(&live));
    for index in 0..2_000 {
        registry
            .projects
            .push(sample_project(&home.path().join(format!("gone-{index}"))));
    }
    let path = home.path().join(".synrepo/projects.toml");
    io::save_to(&path, &registry).unwrap();

    let removed = crate::registry::prune_missing_projects().unwrap();
    assert_eq!(removed.len(), 2_000);
    let saved = io::load_from(&path).unwrap();
    assert_eq!(saved.projects.len(), 1);
    assert_eq!(saved.projects[0].path, live);
}
