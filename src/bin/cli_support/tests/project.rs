use std::fs;

use tempfile::tempdir;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::{
    project_add_output, project_inspect_output, project_list_output, project_prune_missing_output,
    project_remove_output, project_rename_output, project_use_output, resolve_tool_resolution,
    setup_many_resolved,
};

fn home_guard() -> (
    synrepo::test_support::GlobalTestLock,
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = super::support::canonicalize_no_verbatim(home.path());
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
    assert!(list.contains("\"id\""), "{list}");

    let inspect = project_inspect_output(repo.path(), None, true).unwrap();
    assert!(inspect.contains("\"managed\": true"), "{inspect}");
    assert!(inspect.contains("\"uninitialized\""), "{inspect}");
}

#[test]
fn project_use_updates_last_opened_and_reports_identity() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    let entry = synrepo::registry::record_project(repo.path()).unwrap();

    let out = project_use_output(&entry.id).unwrap();

    assert!(out.contains("Project selected"), "{out}");
    assert!(out.contains(&entry.id), "{out}");
    let updated = synrepo::registry::get(repo.path()).unwrap().unwrap();
    assert!(updated.last_opened_at.is_some());
}

#[test]
fn project_rename_changes_display_name_only() {
    let (_lock, _home, _guard) = home_guard();
    let repo = tempdir().unwrap();
    let entry = synrepo::registry::record_project(repo.path()).unwrap();

    let out = project_rename_output(&entry.id, "agent-config").unwrap();

    assert!(out.contains("Project renamed"), "{out}");
    assert!(out.contains("agent-config"), "{out}");
    let updated = synrepo::registry::get(repo.path()).unwrap().unwrap();
    assert_eq!(updated.id, entry.id);
    assert_eq!(updated.path, entry.path);
    assert_eq!(updated.name.as_deref(), Some("agent-config"));
}

#[test]
fn project_use_ambiguous_name_lists_matching_ids() {
    let (_lock, _home, _guard) = home_guard();
    let root = tempdir().unwrap();
    let left = root.path().join("left").join("synrepo");
    let right = root.path().join("right").join("synrepo");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    let first = synrepo::registry::record_project(&left).unwrap();
    let second = synrepo::registry::record_project(&right).unwrap();

    let err = project_use_output("synrepo").unwrap_err();
    let msg = format!("{err:#}");

    assert!(msg.contains("multiple projects match"), "{msg}");
    assert!(msg.contains(&first.id), "{msg}");
    assert!(msg.contains(&second.id), "{msg}");
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
fn project_prune_missing_dry_run_reports_candidates_without_removing() {
    let (_lock, _home, _guard) = home_guard();
    let root = tempdir().unwrap();
    let missing = root.path().join("missing-repo");
    let missing_entry = synrepo::registry::record_project(&missing).unwrap();

    let out = project_prune_missing_output(false, false).unwrap();

    assert!(out.contains("Missing managed projects (1):"), "{out}");
    assert!(out.contains(&missing_entry.id), "{out}");
    assert!(out.contains("Dry run"), "{out}");
    assert!(synrepo::registry::get(&missing).unwrap().is_some());
}

#[test]
fn project_prune_missing_apply_removes_only_missing_projects() {
    let (_lock, _home, _guard) = home_guard();
    let live = tempdir().unwrap();
    let root = tempdir().unwrap();
    let missing = root.path().join("missing-repo");
    synrepo::registry::record_project(live.path()).unwrap();
    synrepo::registry::record_project(&missing).unwrap();

    let out = project_prune_missing_output(true, false).unwrap();

    assert!(
        out.contains("Pruned missing managed projects (1):"),
        "{out}"
    );
    assert!(
        out.contains("No repository state, .synrepo/, global config, or agent files were deleted."),
        "{out}"
    );
    assert!(synrepo::registry::get(&missing).unwrap().is_none());
    assert!(synrepo::registry::get(live.path()).unwrap().is_some());
}

#[test]
fn project_prune_missing_json_reports_apply_state() {
    let (_lock, _home, _guard) = home_guard();
    let root = tempdir().unwrap();
    let missing = root.path().join("missing-repo");
    synrepo::registry::record_project(&missing).unwrap();

    let out = project_prune_missing_output(false, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed["applied"], false);
    assert_eq!(parsed["missing_count"], 1);
    assert_eq!(parsed["missing_projects"][0]["health"]["state"], "missing");
    assert!(synrepo::registry::get(&missing).unwrap().is_some());
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

    setup_many_resolved(repo.path(), &resolution, false, false, false, false).unwrap();
    setup_many_resolved(repo.path(), &resolution, false, false, false, false).unwrap();

    let registry = synrepo::registry::load().unwrap();
    let canonical = synrepo::registry::canonicalize_path(repo.path());
    let matches = registry
        .projects
        .iter()
        .filter(|project| project.path == canonical)
        .count();
    assert_eq!(matches, 1);
}
