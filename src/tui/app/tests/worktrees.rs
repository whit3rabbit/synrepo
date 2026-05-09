//! AppState worktree-discovery toggle tests.

use super::super::*;
use crossterm::event::{KeyCode, KeyModifiers};

fn ready_state() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
    AppState,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempfile::tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let repo = tempfile::tempdir().unwrap();
    crate::bootstrap::bootstrap(repo.path(), None, false).expect("bootstrap");
    let state = AppState::new_poll(
        repo.path(),
        crate::tui::theme::Theme::plain(),
        crate::bootstrap::runtime_probe::AgentIntegration::Absent,
    );
    (lock, repo, guard, state)
}

#[test]
fn quick_actions_include_worktree_toggle_enabled_by_default() {
    let (_lock, _repo, _guard, state) = ready_state();
    let action = state
        .quick_actions
        .iter()
        .find(|action| action.key == "W")
        .expect("worktree quick action");
    assert_eq!(action.label, "disable linked worktrees");
    assert!(action.requires_confirm);
}

#[test]
fn pressing_w_uppercase_disables_worktrees_after_confirmation() {
    let (_lock, repo, _guard, mut state) = ready_state();
    assert!(
        crate::config::Config::load(repo.path())
            .unwrap()
            .include_worktrees
    );

    assert!(state.handle_key(KeyCode::Char('W'), KeyModifiers::NONE));
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::ToggleWorktrees)
    );

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        !crate::config::Config::load(repo.path())
            .unwrap()
            .include_worktrees
    );
    let entry = state.log.as_slice().last().expect("worktrees log entry");
    assert_eq!(entry.tag, "worktrees");
    assert!(entry.message.contains("disabled"));
}

#[test]
fn quick_action_label_flips_after_disable() {
    let (_lock, repo, _guard, mut state) = ready_state();
    let path = repo.path().join(".synrepo/config.toml");
    let mut config = crate::config::Config::load(repo.path()).unwrap();
    config.include_worktrees = false;
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
    state.refresh_now();

    let action = state
        .quick_actions
        .iter()
        .find(|action| action.key == "W")
        .expect("worktree quick action");
    assert_eq!(action.label, "enable linked worktrees");
}
