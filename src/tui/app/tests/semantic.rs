//! AppState embeddings-toggle tests.

use super::super::*;
use crate::tui::actions::{stop_watch, ActionContext, ActionOutcome};
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
    assert!(
        state.quick_actions.iter().all(|action| action.key != "B"),
        "build action should stay hidden until embeddings are enabled"
    );
}

#[test]
#[cfg(feature = "semantic-triage")]
fn pressing_t_enables_embeddings_without_stop_watch_modal() {
    let (_lock, repo, _guard, _home, mut state) = ready_state();

    assert!(state.handle_key(KeyCode::Char('T'), KeyModifiers::NONE));
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::ToggleEmbeddings)
    );
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        state.confirm_stop_watch.is_none(),
        "config-only enable must not ask to stop watch"
    );
    assert!(
        crate::config::Config::load(repo.path())
            .unwrap()
            .enable_semantic_triage
    );
    assert!(
        state.quick_actions.iter().any(|action| action.key == "B"),
        "build action should appear after embeddings are enabled"
    );
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

#[test]
#[cfg(feature = "semantic-triage")]
fn pressing_b_without_watch_queues_embedding_build() {
    let (_lock, repo, _guard, _home, mut state) = ready_state();
    enable_embeddings(repo.path());
    state.refresh_now();

    assert!(state.handle_key(KeyCode::Char('B'), KeyModifiers::NONE));
    assert_eq!(
        state.pending_embedding_build.front(),
        Some(&PendingEmbeddingBuild {
            stopped_watch: false
        })
    );
}

#[test]
#[cfg(feature = "semantic-triage")]
fn pressing_b_with_watch_opens_stop_watch_modal() {
    let _watch_guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (_lock, repo, _guard, _home, mut state) = ready_state();
    enable_embeddings(repo.path());
    state.refresh_now();
    let ctx = ActionContext::new(repo.path());
    let start = crate::tui::actions::start_watch_daemon(&ctx);
    assert!(
        matches!(start, ActionOutcome::Ack { .. }),
        "setup start must succeed, got {start:?}"
    );

    assert!(state.handle_key(KeyCode::Char('B'), KeyModifiers::NONE));
    assert_eq!(
        state.confirm_stop_watch.as_ref().map(|s| &s.pending),
        Some(&PendingStopWatchAction::BuildEmbeddings)
    );
    assert!(state.pending_embedding_build.is_empty());

    let stop = stop_watch(&ctx);
    assert!(
        matches!(
            stop,
            ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
        ),
        "cleanup stop must succeed, got {stop:?}"
    );
}

#[test]
#[cfg(feature = "semantic-triage")]
fn confirming_embedding_stop_watch_queues_build() {
    let _watch_guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (_lock, repo, _guard, _home, mut state) = ready_state();
    enable_embeddings(repo.path());
    state.refresh_now();
    let ctx = ActionContext::new(repo.path());
    let start = crate::tui::actions::start_watch_daemon(&ctx);
    assert!(
        matches!(start, ActionOutcome::Ack { .. }),
        "setup start must succeed, got {start:?}"
    );

    assert!(state.handle_key(KeyCode::Char('B'), KeyModifiers::NONE));
    assert!(state.handle_key(KeyCode::Char('y'), KeyModifiers::NONE));
    assert!(state.confirm_stop_watch.is_none());
    assert_eq!(
        state.pending_embedding_build.front(),
        Some(&PendingEmbeddingBuild {
            stopped_watch: true
        })
    );
    assert!(matches!(
        crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir),
        crate::pipeline::watch::WatchServiceStatus::Inactive
    ));
}

#[test]
#[cfg(not(feature = "semantic-triage"))]
fn enabling_without_semantic_feature_does_not_stop_watch() {
    let _watch_guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (_lock, repo, _guard, _home, mut state) = ready_state();
    let ctx = ActionContext::new(repo.path());
    let start = crate::tui::actions::start_watch_daemon(&ctx);
    assert!(
        matches!(start, ActionOutcome::Ack { .. }),
        "setup start must succeed, got {start:?}"
    );

    assert!(state.handle_key(KeyCode::Char('T'), KeyModifiers::NONE));
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::ToggleEmbeddings)
    );
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        state.confirm_stop_watch.is_none(),
        "missing feature must fail before opening stop-watch modal"
    );
    assert!(state.log.as_slice().iter().any(|entry| {
        entry.tag == "embeddings" && entry.message.contains("not built with `semantic-triage`")
    }));
    assert!(matches!(
        crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir),
        crate::pipeline::watch::WatchServiceStatus::Running(_)
    ));

    let stop = stop_watch(&ctx);
    assert!(
        matches!(
            stop,
            ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
        ),
        "cleanup stop must succeed, got {stop:?}"
    );
}

#[cfg(feature = "semantic-triage")]
fn enable_embeddings(repo: &std::path::Path) {
    let path = repo.join(".synrepo/config.toml");
    let mut config = crate::config::Config::load(repo).unwrap();
    config.enable_semantic_triage = true;
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
}
