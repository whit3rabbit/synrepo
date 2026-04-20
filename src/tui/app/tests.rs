//! AppState tests: welcome banner, event draining, reconcile-active tracking,
//! tab switching, scroll/follow handling, and spinner frame advancement.

use super::*;
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::watch::{ReconcileOutcome, WatchEvent};
use crate::tui::theme::Theme;

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
    assert_eq!(state.active_tab, ActiveTab::Actions);
    state.handle_key(KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Live);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Health);
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
