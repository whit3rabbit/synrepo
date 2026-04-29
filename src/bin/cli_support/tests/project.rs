use std::fs;

use tempfile::tempdir;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::{
    project_add_output, project_inspect_output, project_list_output, project_remove_output,
    resolve_tool_resolution, setup_many_resolved,
};

fn home_guard() -> (
    synrepo::test_support::GlobalTestLock,
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = std::fs::canonicalize(home.path()).unwrap();
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    (lock, home, guard)
}

#[test]
fn project_add_initializes_and_registers_repo() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn project_add() {}\n").unwrap();

    let out = project_add_output(repo.path(), None).unwrap();

    assert!(out.contains("Project registered"), "{out}");
    assert!(repo.path().join(".synrepo").exists());
    assert!(synrepo::registry::get(repo.path()).unwrap().is_some());
}

#[test]
fn project_list_and_inspect_json_include_health() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    synrepo::registry::record_project(repo.path()).unwrap();

    let list = project_list_output(true).unwrap();
    assert!(list.contains("\"projects\""), "{list}");
    assert!(list.contains("\"health\""), "{list}");

    let inspect = project_inspect_output(repo.path(), None, true).unwrap();
    assert!(inspect.contains("\"managed\": true"), "{inspect}");
    assert!(inspect.contains("\"uninitialized\""), "{inspect}");
}

#[test]
fn project_inspect_unmanaged_suggests_add() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();

    let out = project_inspect_output(repo.path(), None, false).unwrap();

    assert!(out.contains("Unmanaged project"), "{out}");
    assert!(out.contains("synrepo project add"), "{out}");
}

#[test]
fn project_remove_only_unregisters_registry_entry() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".synrepo")).unwrap();
    synrepo::registry::record_project(repo.path()).unwrap();

    let out = project_remove_output(repo.path(), None).unwrap();

    assert!(out.contains("Project unmanaged"), "{out}");
    assert!(repo.path().join(".synrepo").exists());
    assert!(synrepo::registry::get(repo.path()).unwrap().is_none());
}

#[test]
fn scripted_setup_registers_project_once() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn setup_registers() {}\n",
    )
    .unwrap();
    let resolution = resolve_tool_resolution(Some(AgentTool::Claude), &[], &[]).unwrap();

    setup_many_resolved(repo.path(), &resolution, false, false, false).unwrap();
    setup_many_resolved(repo.path(), &resolution, false, false, false).unwrap();

    let registry = synrepo::registry::load().unwrap();
    let canonical = synrepo::registry::canonicalize_path(repo.path());
    let matches = registry
        .projects
        .iter()
        .filter(|project| project.path == canonical)
        .count();
    assert_eq!(matches, 1);
}
