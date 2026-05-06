//! AppState embeddings-toggle tests.

use super::super::*;
use crossterm::event::{KeyCode, KeyModifiers};

fn ready_state() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
    tempfile::TempDir,
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
    (lock, repo, guard, home, state)
}

#[test]
fn quick_actions_include_embeddings_toggle_when_initialized() {
    let (_lock, _repo, _guard, _home, state) = ready_state();
    let action = state
        .quick_actions
        .iter()
        .find(|action| action.key == "T")
        .expect("embeddings quick action");
    assert_eq!(action.label, "enable optional embeddings");
    assert!(action.requires_confirm);
}

#[test]
fn pressing_t_disables_embeddings_after_confirmation() {
    let (_lock, repo, _guard, _home, mut state) = ready_state();
    let path = repo.path().join(".synrepo/config.toml");
    let mut config = crate::config::Config::load(repo.path()).unwrap();
    config.enable_semantic_triage = true;
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
    state.refresh_now();

    let consumed = state.handle_key(KeyCode::Char('T'), KeyModifiers::NONE);
    assert!(consumed, "T should open embeddings confirmation");
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::ToggleEmbeddings)
    );

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        !crate::config::Config::load(repo.path())
            .unwrap()
            .enable_semantic_triage
    );
    let entry = state.log.as_slice().last().expect("embeddings log entry");
    assert_eq!(entry.tag, "embeddings");
    assert!(entry.message.contains("disabled"));
}
