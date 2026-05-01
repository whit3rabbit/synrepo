use super::super::*;
use super::support::{isolated_home, make_live_state, make_poll_state, make_ready_poll_state};
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::tui::actions::{stop_watch, ActionContext, ActionOutcome};
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn pressing_r_sets_refresh_toast() {
    // The 'r' refresh used to be a silent no-op when nothing on disk had
    // changed. Now it always sets a toast so the operator sees confirmation.
    let mut state = make_poll_state();
    assert!(state.active_toast().is_none(), "fresh state has no toast");
    let consumed = state.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
    assert!(consumed, "'r' should consume the key event");
    let toast = state.active_toast().expect("toast must be set after 'r'");
    assert!(
        toast.starts_with("refreshed"),
        "toast should announce a refresh: {toast:?}"
    );
}

#[test]
fn pressing_w_starts_watch_sets_toast_log_and_refreshes_snapshot() {
    let _guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (tempdir, mut state) = make_ready_poll_state();
    assert_eq!(state.watch_toggle_label(), Some("start"));

    let consumed = state.handle_key(KeyCode::Char('w'), KeyModifiers::NONE);
    assert!(consumed, "'w' should consume the key event in poll mode");
    assert_eq!(state.watch_toggle_label(), Some("stop"));
    assert!(
        matches!(
            state
                .snapshot
                .diagnostics
                .as_ref()
                .map(|diag| &diag.watch_status),
            Some(crate::pipeline::watch::WatchServiceStatus::Running(_))
        ),
        "watch should be running after toggle"
    );
    let toast = state
        .active_toast()
        .expect("watch toggle should set a toast");
    assert!(
        toast.contains("watch") || toast.contains("spawned"),
        "watch-start toast should mention the action: {toast:?}"
    );
    let entry = state
        .log
        .as_slice()
        .last()
        .expect("watch toggle should log");
    assert_eq!(entry.tag, "watch");

    let outcome = stop_watch(&ActionContext::new(tempdir.path()));
    assert!(
        matches!(
            outcome,
            ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
        ),
        "cleanup stop must succeed, got {outcome:?}"
    );
}

#[test]
fn pressing_w_stops_watch_from_actions_tab_and_sets_feedback() {
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
    state.set_tab(ActiveTab::Actions);
    let consumed = state.handle_key(KeyCode::Char('w'), KeyModifiers::NONE);
    assert!(consumed, "'w' should work outside the Live tab");
    assert_eq!(state.watch_toggle_label(), Some("start"));
    assert!(
        matches!(
            state
                .snapshot
                .diagnostics
                .as_ref()
                .map(|diag| &diag.watch_status),
            Some(crate::pipeline::watch::WatchServiceStatus::Inactive)
        ),
        "watch should be inactive after stop"
    );
    let toast = state.active_toast().expect("watch stop should set a toast");
    assert!(
        toast.contains("stop") || toast.contains("Stopped") || toast.contains("stopping"),
        "watch-stop toast should mention the action: {toast:?}"
    );
    let entry = state.log.as_slice().last().expect("watch stop should log");
    assert_eq!(entry.tag, "watch");
}

#[test]
fn pressing_w_is_ignored_in_live_mode() {
    let (mut state, _tx) = make_live_state();
    let consumed = state.handle_key(KeyCode::Char('w'), KeyModifiers::NONE);
    assert!(
        !consumed,
        "live dashboard should keep foreground watch behavior unchanged"
    );
    assert!(state.active_toast().is_none());
    assert!(state.log.as_slice().is_empty());
}
