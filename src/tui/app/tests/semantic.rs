//! AppState embeddings-toggle tests.

use super::super::*;
use super::support::{isolated_home, make_ready_poll_state};
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn quick_actions_include_embeddings_toggle_when_initialized() {
    let (_repo, state) = make_ready_poll_state();
    let action = state
        .quick_actions
        .iter()
        .find(|action| action.key == "T")
        .expect("embeddings quick action");
    assert_eq!(action.label, "enable embeddings");
    assert!(action.requires_confirm);
}

#[test]
fn pressing_t_disables_embeddings_after_confirmation() {
    let (_home, _home_guard) = isolated_home();
    let (repo, mut state) = make_ready_poll_state();
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
