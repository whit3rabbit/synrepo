use crossterm::event::{KeyCode, KeyModifiers};
use tempfile::tempdir;

use super::*;
use crate::tui::app::{
    ConfirmStopWatchState, ExplainMode, PendingExplainRun, PendingStopWatchAction,
};

#[path = "picker_tests.rs"]
mod picker_tests;

fn home_guard() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (lock, home, guard)
}

fn make_partial_project(path: &std::path::Path) {
    std::fs::create_dir_all(path.join(".synrepo")).unwrap();
}

#[test]
fn switch_project_clears_transients_and_preserves_project_states() {
    let (_lock, _home, _guard) = home_guard();
    let first = tempdir().unwrap();
    let second = tempdir().unwrap();
    make_partial_project(first.path());
    make_partial_project(second.path());
    let first_entry = registry::record_project(first.path()).unwrap();
    let second_entry = registry::record_project(second.path()).unwrap();
    let mut state = GlobalAppState::new(first.path(), Theme::plain(), true).unwrap();

    state.switch_project(&first_entry.id).unwrap();
    {
        let active = state.active_state_mut().unwrap();
        active.pending_explain.push_back(PendingExplainRun {
            mode: ExplainMode::AllStale,
            stopped_watch: false,
        });
        active.confirm_stop_watch = Some(ConfirmStopWatchState {
            pending: PendingStopWatchAction::Explain(ExplainMode::Changed),
        });
    }

    state.switch_project(&second_entry.id).unwrap();

    let first_state = state.project_states.get(&first_entry.id).unwrap();
    assert!(first_state.pending_explain.is_empty());
    assert!(first_state.confirm_stop_watch.is_none());
    assert!(state.project_states.contains_key(&second_entry.id));
    assert_eq!(
        state.active_project_id.as_deref(),
        Some(second_entry.id.as_str())
    );
    let active = state.active_state().unwrap();
    let active_name = active.project_name.as_ref().unwrap();
    assert!(
        active.header_vm.repo_display.starts_with(active_name),
        "cached header should include active project name: {:?}",
        active.header_vm.repo_display
    );
}

#[test]
fn picker_enter_switches_to_selected_project() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    make_partial_project(project.path());
    let entry = registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), true).unwrap();

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(state.active_project_id.as_deref(), Some(entry.id.as_str()));
    assert!(state.picker.is_none());
}

#[test]
fn picker_rename_updates_alias_only() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    make_partial_project(project.path());
    let entry = registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), true).unwrap();

    state.picker.as_mut().unwrap().rename_input = Some("agent-config".to_string());
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    let renamed = registry::resolve_project(&entry.id).unwrap();
    assert_eq!(renamed.id, entry.id);
    assert_eq!(renamed.path, entry.path);
    assert_eq!(renamed.name.as_deref(), Some("agent-config"));
}

#[test]
fn p_key_opens_explore_tab_for_active_project() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    make_partial_project(project.path());
    registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), false).unwrap();

    assert!(state.handle_key(KeyCode::Char('p'), KeyModifiers::NONE));

    assert_eq!(
        state.active_state().unwrap().active_tab,
        crate::tui::app::ActiveTab::Repos
    );
    assert!(state.picker.is_none());
}

#[test]
fn explore_enter_switches_selected_project() {
    let (_lock, home, _guard) = home_guard();
    let alpha = home.path().join("alpha");
    let beta = home.path().join("beta");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::create_dir_all(&beta).unwrap();
    make_partial_project(&alpha);
    make_partial_project(&beta);
    let alpha_entry = registry::record_project(&alpha).unwrap();
    let beta_entry = registry::record_project(&beta).unwrap();
    registry::rename_project(&alpha_entry.id, "alpha").unwrap();
    registry::rename_project(&beta_entry.id, "beta").unwrap();
    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();

    state.open_explore_tab();
    state.explore_selected = state
        .projects
        .iter()
        .position(|project| project.id == beta_entry.id)
        .unwrap();
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        state.active_project_id.as_deref(),
        Some(beta_entry.id.as_str())
    );
    assert_eq!(
        state.active_state().unwrap().repo_root,
        beta.canonicalize().unwrap()
    );
}

#[test]
fn explore_refresh_preserves_selected_project() {
    let (_lock, home, _guard) = home_guard();
    let alpha = home.path().join("alpha");
    let beta = home.path().join("beta");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::create_dir_all(&beta).unwrap();
    make_partial_project(&alpha);
    make_partial_project(&beta);
    let alpha_entry = registry::record_project(&alpha).unwrap();
    let beta_entry = registry::record_project(&beta).unwrap();
    registry::rename_project(&alpha_entry.id, "alpha").unwrap();
    registry::rename_project(&beta_entry.id, "beta").unwrap();
    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();

    state.open_explore_tab();
    state.explore_selected = state
        .projects
        .iter()
        .position(|project| project.id == beta_entry.id)
        .unwrap();
    assert!(state.handle_key(KeyCode::Char('r'), KeyModifiers::NONE));

    assert_eq!(
        state.projects[state.explore_selected_index()].id.as_str(),
        beta_entry.id.as_str()
    );
}

#[test]
fn explore_watch_toggle_scopes_to_selected_project() {
    let (_lock, home, _guard) = home_guard();
    let alpha = home.path().join("alpha");
    let beta = home.path().join("beta");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::create_dir_all(&beta).unwrap();
    make_partial_project(&alpha);
    make_partial_project(&beta);
    let alpha_entry = registry::record_project(&alpha).unwrap();
    let beta_entry = registry::record_project(&beta).unwrap();
    registry::rename_project(&alpha_entry.id, "alpha").unwrap();
    registry::rename_project(&beta_entry.id, "beta").unwrap();

    let alpha_state = crate::pipeline::watch::watch_daemon_state_path(
        &crate::config::Config::synrepo_dir(&alpha),
    );
    let beta_state =
        crate::pipeline::watch::watch_daemon_state_path(&crate::config::Config::synrepo_dir(&beta));
    std::fs::create_dir_all(alpha_state.parent().unwrap()).unwrap();
    std::fs::create_dir_all(beta_state.parent().unwrap()).unwrap();
    std::fs::write(&alpha_state, b"{not json").unwrap();
    std::fs::write(&beta_state, b"{not json").unwrap();

    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();
    state.open_explore_tab();
    state.explore_selected = state
        .projects
        .iter()
        .position(|project| project.id == beta_entry.id)
        .unwrap();

    assert!(state.handle_key(KeyCode::Char('w'), KeyModifiers::NONE));

    assert!(
        alpha_state.exists(),
        "watch toggle must not clean the non-selected project"
    );
    assert!(
        !beta_state.exists(),
        "watch toggle should clean only the selected project's corrupt artifact"
    );
    assert_eq!(
        state.projects[state.explore_selected_index()].id.as_str(),
        beta_entry.id.as_str()
    );
}

#[test]
fn load_project_refs_hides_uninitialized_and_keeps_ready_and_partial() {
    let (_lock, home, _guard) = home_guard();
    let ready = home.path().join("ready");
    let partial = home.path().join("partial");
    let uninitialized = home.path().join("uninitialized");
    std::fs::create_dir_all(&ready).unwrap();
    std::fs::create_dir_all(&partial).unwrap();
    std::fs::create_dir_all(&uninitialized).unwrap();
    std::fs::write(ready.join("README.md"), "ready token\n").unwrap();
    crate::bootstrap::bootstrap(&ready, None, false).unwrap();
    make_partial_project(&partial);
    let ready_entry = registry::record_project(&ready).unwrap();
    let partial_entry = registry::record_project(&partial).unwrap();
    let uninitialized_entry = registry::record_project(&uninitialized).unwrap();

    let refs = load_project_refs().unwrap();
    let ids = refs
        .iter()
        .map(|project| project.id.as_str())
        .collect::<Vec<_>>();

    assert!(ids.contains(&ready_entry.id.as_str()), "{ids:?}");
    assert!(ids.contains(&partial_entry.id.as_str()), "{ids:?}");
    assert!(!ids.contains(&uninitialized_entry.id.as_str()), "{ids:?}");
}

#[test]
fn load_project_refs_hides_bootstrap_only_entries() {
    let (_lock, home, _guard) = home_guard();
    let bootstrap_only = home.path().join("bootstrap-only");
    let registered = home.path().join("registered");
    std::fs::create_dir_all(&bootstrap_only).unwrap();
    std::fs::create_dir_all(&registered).unwrap();
    std::fs::write(bootstrap_only.join("README.md"), "bootstrap only\n").unwrap();
    std::fs::write(registered.join("README.md"), "registered\n").unwrap();
    crate::bootstrap::bootstrap(&bootstrap_only, None, false).unwrap();
    crate::bootstrap::bootstrap(&registered, None, false).unwrap();
    let bootstrap_only_entry = registry::get(&bootstrap_only).unwrap().unwrap();
    let registered_entry = registry::record_project(&registered).unwrap();

    let refs = load_project_refs().unwrap();
    let ids = refs
        .iter()
        .map(|project| project.id.as_str())
        .collect::<Vec<_>>();

    assert!(!ids.contains(&bootstrap_only_entry.id.as_str()), "{ids:?}");
    assert!(ids.contains(&registered_entry.id.as_str()), "{ids:?}");
}
