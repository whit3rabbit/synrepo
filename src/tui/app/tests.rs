//! AppState tests: welcome banner, event draining, reconcile-active tracking,
//! tab switching, scroll/follow handling, and spinner frame advancement.

use super::*;
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::watch::{ReconcileOutcome, WatchEvent};
use crate::tui::actions::{stop_watch, ActionContext, ActionOutcome};
use crate::tui::probe::Severity;
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyModifiers};

fn make_poll_state() -> AppState {
    let tempdir = tempfile::tempdir().unwrap();
    AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent)
}

fn make_live_state() -> (AppState, crossbeam_channel::Sender<WatchEvent>) {
    let tempdir = tempfile::tempdir().unwrap();
    let (tx, rx) = crossbeam_channel::bounded::<WatchEvent>(16);
    let state = AppState::new_live(tempdir.path(), Theme::plain(), AgentIntegration::Absent, rx);
    // Keep the tempdir alive via Box::leak so the caller doesn't have to manage it.
    // Safe: these are short-lived unit tests.
    std::mem::forget(tempdir);
    (state, tx)
}

fn make_ready_poll_state() -> (tempfile::TempDir, AppState) {
    let tempdir = tempfile::tempdir().unwrap();
    crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");
    let state = AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
    (tempdir, state)
}

fn isolated_home() -> (tempfile::TempDir, crate::config::test_home::HomeEnvGuard) {
    let home = tempfile::tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard)
}

#[test]
fn new_poll_starts_with_empty_log() {
    let state = make_poll_state();
    assert!(state.log.as_slice().is_empty());
}

#[test]
fn new_poll_defaults_to_live_tab_with_follow_on() {
    let state = make_poll_state();
    assert_eq!(state.active_tab, ActiveTab::Live);
    assert!(state.follow_mode);
    assert_eq!(state.scroll_offset, 0);
    assert!(!state.reconcile_active);
    assert_eq!(state.frame, 0);
}

#[test]
fn push_welcome_banner_seeds_exactly_one_entry() {
    let mut state = make_poll_state();
    state.push_welcome_banner();
    let entries = state.log.as_slice();
    assert_eq!(entries.len(), 1);
    let banner = &entries[0];
    assert_eq!(banner.tag, "synrepo");
    assert!(
        banner.message.to_ascii_lowercase().contains("welcome"),
        "banner message should greet the user: {:?}",
        banner.message
    );
    assert!(matches!(banner.severity, Severity::Healthy));
}

#[test]
fn push_welcome_banner_is_idempotent_per_call_but_caller_must_only_invoke_once() {
    // The state machine itself does not dedupe — the caller is responsible
    // for the one-shot property.
    let mut state = make_poll_state();
    state.push_welcome_banner();
    state.push_welcome_banner();
    assert_eq!(state.log.as_slice().len(), 2);
}

#[test]
fn drain_events_pulls_all_pending_into_log() {
    let (mut state, tx) = make_live_state();
    tx.send(WatchEvent::ReconcileStarted {
        at: "t0".to_string(),
        triggering_events: 0,
    })
    .unwrap();
    tx.send(WatchEvent::Error {
        at: "t1".to_string(),
        message: "x".to_string(),
    })
    .unwrap();
    state.drain_events();
    let log = state.log.as_slice();
    assert_eq!(log.len(), 2);
    assert!(log.iter().all(|e| e.tag == "watch"));
}

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
        toast.starts_with("Refreshed"),
        "toast should announce a refresh: {toast:?}"
    );
}

#[test]
fn pressing_w_starts_watch_sets_toast_log_and_refreshes_snapshot() {
    let _guard = crate::test_support::global_test_lock("tui-app-watch-toggle");
    let (_home, _home_guard) = isolated_home();
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

#[test]
fn drain_events_is_noop_in_poll_mode() {
    let mut state = make_poll_state();
    state.drain_events();
    assert!(state.log.as_slice().is_empty());
}

#[test]
fn handle_key_switches_tabs() {
    let mut state = make_poll_state();
    state.handle_key(KeyCode::Char('2'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Health);
    state.handle_key(KeyCode::Char('3'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Explain);
    assert!(
        state.explain_preview.is_some(),
        "entering the Explain tab should load the inline preview"
    );
    state.handle_key(KeyCode::Char('4'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Actions);
    state.handle_key(KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Live);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Health);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Explain);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Actions);
}

#[test]
fn scroll_up_holds_offset_against_new_entries() {
    let (mut state, tx) = make_live_state();
    // Start with follow on, press k once.
    assert!(state.follow_mode);
    state.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
    assert!(!state.follow_mode);
    assert_eq!(state.scroll_offset, 1);
    // New reconcile event should not change offset or follow state.
    tx.send(WatchEvent::ReconcileStarted {
        at: "t0".to_string(),
        triggering_events: 0,
    })
    .unwrap();
    state.drain_events();
    assert_eq!(state.scroll_offset, 1);
    assert!(!state.follow_mode);
}

#[test]
fn end_key_restores_follow_mode() {
    let mut state = make_poll_state();
    state.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
    state.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
    assert!(!state.follow_mode);
    assert!(state.scroll_offset >= 2);
    state.handle_key(KeyCode::End, KeyModifiers::NONE);
    assert!(state.follow_mode);
    assert_eq!(state.scroll_offset, 0);
}

#[test]
fn scroll_keys_ignored_outside_live_tab() {
    let mut state = make_poll_state();
    state.set_tab(ActiveTab::Health);
    let consumed = state.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
    assert!(!consumed, "scroll keys must not fire on Health tab");
    assert_eq!(state.scroll_offset, 0);
    assert!(state.follow_mode);
}

#[test]
fn reconcile_active_toggles_on_started_and_finished() {
    use crate::pipeline::structural::CompileSummary;
    let (mut state, tx) = make_live_state();
    assert!(!state.reconcile_active);
    tx.send(WatchEvent::ReconcileStarted {
        at: "t0".to_string(),
        triggering_events: 0,
    })
    .unwrap();
    state.drain_events();
    assert!(state.reconcile_active);
    tx.send(WatchEvent::ReconcileFinished {
        at: "t1".to_string(),
        outcome: ReconcileOutcome::Completed(CompileSummary::default()),
        triggering_events: 0,
    })
    .unwrap();
    state.drain_events();
    assert!(!state.reconcile_active);
}

#[test]
fn reconcile_active_clears_on_error_event() {
    let (mut state, tx) = make_live_state();
    tx.send(WatchEvent::ReconcileStarted {
        at: "t0".to_string(),
        triggering_events: 0,
    })
    .unwrap();
    state.drain_events();
    assert!(state.reconcile_active);
    tx.send(WatchEvent::Error {
        at: "t1".to_string(),
        message: "boom".to_string(),
    })
    .unwrap();
    state.drain_events();
    assert!(!state.reconcile_active);
}

#[test]
fn frame_counter_advances_on_tick() {
    let mut state = make_poll_state();
    let before = state.frame;
    state.tick();
    state.tick();
    assert!(
        state.frame > before,
        "frame should advance every tick: {before} -> {}",
        state.frame
    );
}

#[test]
fn page_down_at_bottom_reenables_follow() {
    let mut state = make_poll_state();
    state.handle_key(KeyCode::PageUp, KeyModifiers::NONE);
    assert!(!state.follow_mode);
    // Enough PageDn hits to saturate back to 0.
    state.handle_key(KeyCode::PageDown, KeyModifiers::NONE);
    state.handle_key(KeyCode::PageDown, KeyModifiers::NONE);
    assert_eq!(state.scroll_offset, 0);
    assert!(state.follow_mode);
}

fn prime_picker(state: &mut AppState) {
    state.picker = Some(FolderPickerState {
        folders: vec![
            FolderEntry {
                path: "src/".to_string(),
                indexable_count: 2,
                supported_count: 2,
                checked: true,
            },
            FolderEntry {
                path: "docs/".to_string(),
                indexable_count: 1,
                supported_count: 0,
                checked: false,
            },
        ],
        cursor: 0,
    });
}

#[test]
fn picker_esc_clears_without_exit() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    let consumed = state.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert!(consumed);
    assert!(state.picker.is_none());
    assert!(!state.should_exit, "Esc in picker must not exit the loop");
    assert!(state.pending_explain.is_none());
}

#[test]
fn picker_enter_with_selection_queues_paths_mode() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    let consumed = state.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(consumed);
    assert!(!state.should_exit);
    match &state.pending_explain {
        Some(PendingExplainRun {
            mode: ExplainMode::Paths(paths),
            stopped_watch: false,
        }) => {
            assert_eq!(paths, &vec!["src/".to_string()]);
        }
        other => panic!("expected Paths(..), got {other:?}"),
    }
    assert!(
        state.picker.is_none(),
        "picker cleared on successful commit"
    );
}

#[test]
fn picker_enter_with_empty_selection_stays_open() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    // Uncheck the only checked entry.
    state.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    let consumed = state.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(consumed);
    assert!(
        state.picker.is_some(),
        "empty-selection Enter must keep picker open"
    );
    assert!(!state.should_exit);
    assert!(state.pending_explain.is_none());
}

#[test]
fn picker_navigation_and_toggle_move_cursor() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    // Start at row 0. Down should move to row 1.
    state.handle_key(KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(state.picker.as_ref().unwrap().cursor, 1);
    // Space toggles docs/.
    state.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    let entry = &state.picker.as_ref().unwrap().folders[1];
    assert_eq!(entry.path, "docs/");
    assert!(entry.checked);
    // `k` moves back up.
    state.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
    assert_eq!(state.picker.as_ref().unwrap().cursor, 0);
}

#[test]
fn picker_tab_switch_clears_it() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    state.handle_key(KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Live);
    assert!(state.picker.is_none());
}

#[test]
fn picker_quit_key_falls_through() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    let consumed = state.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(consumed);
    assert!(state.should_exit, "q must quit even with picker open");
}

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
