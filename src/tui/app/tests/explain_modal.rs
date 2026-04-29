use super::super::*;
use super::support::{isolated_home, make_ready_poll_state};
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::tui::actions::{stop_watch, ActionContext, ActionOutcome};
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyModifiers};
use std::fs;

#[test]
fn queue_explain_without_watch_sets_pending_run() {
    let (_repo, mut state) = make_ready_poll_state();
    state.queue_explain(ExplainMode::AllStale);
    assert!(
        state.confirm_stop_watch.is_none(),
        "no watch running, modal must not open"
    );
    assert!(matches!(
        state.pending_explain,
        Some(PendingExplainRun {
            mode: ExplainMode::AllStale,
            stopped_watch: false,
        })
    ));
    assert!(!state.should_exit, "explain runs inside the dashboard");
}

#[test]
fn queue_explain_with_watch_opens_confirm_modal() {
    let _guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (_home, _home_guard) = isolated_home();
    let tempdir = tempfile::tempdir().unwrap();
    crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");
    let ctx = ActionContext::new(tempdir.path());
    let start = crate::tui::actions::start_watch_daemon(&ctx);
    assert!(
        matches!(start, ActionOutcome::Ack { .. }),
        "setup start must succeed, got {start:?}"
    );

    let mut state = AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
    state.queue_explain(ExplainMode::Changed);
    assert!(
        state.confirm_stop_watch.is_some(),
        "watch running must open confirm modal"
    );
    assert!(
        state.pending_explain.is_none(),
        "launch must be gated on confirm modal"
    );
    assert!(!state.should_exit, "modal open, must not exit yet");
    let pending = state.confirm_stop_watch.as_ref().unwrap();
    assert_eq!(pending.pending_mode, ExplainMode::Changed);

    // Cleanup: stop the daemon before the tempdir drops.
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
fn confirm_modal_y_stops_watch_and_queues_explain() {
    let _guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (_home, _home_guard) = isolated_home();
    let tempdir = tempfile::tempdir().unwrap();
    crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");
    let ctx = ActionContext::new(tempdir.path());
    let start = crate::tui::actions::start_watch_daemon(&ctx);
    assert!(
        matches!(start, ActionOutcome::Ack { .. }),
        "setup start must succeed, got {start:?}"
    );

    let mut state = AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
    state.queue_explain(ExplainMode::AllStale);
    assert!(state.confirm_stop_watch.is_some());

    let consumed = state.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(consumed);
    assert!(!state.should_exit, "explain runs inside the dashboard");
    assert!(matches!(
        state.pending_explain,
        Some(PendingExplainRun {
            mode: ExplainMode::AllStale,
            stopped_watch: true,
        })
    ));
    assert!(
        state.confirm_stop_watch.is_none(),
        "modal cleared after commit"
    );
}

#[test]
fn confirm_modal_n_cancels_without_stopping_watch() {
    let _guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (_home, _home_guard) = isolated_home();
    let tempdir = tempfile::tempdir().unwrap();
    crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");
    let ctx = ActionContext::new(tempdir.path());
    let start = crate::tui::actions::start_watch_daemon(&ctx);
    assert!(matches!(start, ActionOutcome::Ack { .. }));

    let mut state = AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
    state.queue_explain(ExplainMode::AllStale);
    assert!(state.confirm_stop_watch.is_some());

    let consumed = state.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(consumed);
    assert!(state.confirm_stop_watch.is_none(), "n clears the modal");
    assert!(!state.should_exit);
    assert!(state.pending_explain.is_none());

    // Watch must still be running; n is a pure cancel.
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

#[test]
fn confirm_modal_5_switches_to_actions_and_clears_modal() {
    let (_repo, mut state) = make_ready_poll_state();
    state.confirm_stop_watch = Some(ConfirmStopWatchState {
        pending_mode: ExplainMode::AllStale,
    });

    let consumed = state.handle_key(KeyCode::Char('5'), KeyModifiers::NONE);

    assert!(consumed);
    assert_eq!(state.active_tab, ActiveTab::Actions);
    assert!(state.confirm_stop_watch.is_none());
    assert!(state.pending_explain.is_none());
}

#[test]
fn explain_tab_docs_export_does_not_queue_model_run() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);

    let consumed = state.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);

    assert!(consumed);
    assert!(state.pending_explain.is_none());
    assert!(state.confirm_stop_watch.is_none());
    assert!(
        state
            .log
            .as_slice()
            .iter()
            .any(|entry| entry.message.contains("docs exported")),
        "docs export should be logged"
    );
}

#[test]
fn explain_tab_docs_clean_previews_before_apply() {
    let (repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    let docs_dir = repo
        .path()
        .join(".synrepo")
        .join("explain-docs")
        .join("files");
    let index_dir = repo.path().join(".synrepo").join("explain-index");
    fs::create_dir_all(&docs_dir).unwrap();
    fs::create_dir_all(&index_dir).unwrap();
    let doc_path = docs_dir.join("file_demo.md");
    let index_path = index_dir.join("index.dat");
    fs::write(&doc_path, "demo").unwrap();
    fs::write(&index_path, "demo").unwrap();

    assert!(state.handle_key(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(doc_path.exists(), "clean preview must preserve docs");
    assert!(index_path.exists(), "clean preview must preserve index");

    assert!(state.handle_key(KeyCode::Char('X'), KeyModifiers::NONE));
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::DocsCleanApply)
    );
    assert!(doc_path.exists(), "clean apply must wait for confirm");
    assert!(index_path.exists(), "clean apply must wait for confirm");

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!doc_path.exists(), "clean apply must remove docs");
    assert!(!index_path.exists(), "clean apply must remove index");
}
