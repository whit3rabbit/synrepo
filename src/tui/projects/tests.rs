use crossterm::event::{KeyCode, KeyModifiers};
use tempfile::tempdir;

use super::*;
use crate::tui::app::{ConfirmStopWatchState, ExplainMode, PendingExplainRun};

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

#[test]
fn switch_project_clears_transients_and_preserves_project_states() {
    let (_lock, _home, _guard) = home_guard();
    let first = tempdir().unwrap();
    let second = tempdir().unwrap();
    let first_entry = registry::record_project(first.path()).unwrap();
    let second_entry = registry::record_project(second.path()).unwrap();
    let mut state = GlobalAppState::new(first.path(), Theme::plain(), true).unwrap();

    state.switch_project(&first_entry.id).unwrap();
    {
        let active = state.active_state_mut().unwrap();
        active.pending_explain = Some(PendingExplainRun {
            mode: ExplainMode::AllStale,
            stopped_watch: false,
        });
        active.confirm_stop_watch = Some(ConfirmStopWatchState {
            pending_mode: ExplainMode::Changed,
        });
    }

    state.switch_project(&second_entry.id).unwrap();

    let first_state = state.project_states.get(&first_entry.id).unwrap();
    assert!(first_state.pending_explain.is_none());
    assert!(first_state.confirm_stop_watch.is_none());
    assert!(state.project_states.contains_key(&second_entry.id));
    assert_eq!(
        state.active_project_id.as_deref(),
        Some(second_entry.id.as_str())
    );
}

#[test]
fn picker_enter_switches_to_selected_project() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
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
    let entry = registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), true).unwrap();

    state.picker.as_mut().unwrap().rename_input = Some("agent-config".to_string());
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    let renamed = registry::resolve_project(&entry.id).unwrap();
    assert_eq!(renamed.id, entry.id);
    assert_eq!(renamed.path, entry.path);
    assert_eq!(renamed.name.as_deref(), Some("agent-config"));
}
