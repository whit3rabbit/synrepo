use super::super::*;
use super::support::{isolated_home, make_live_state, make_poll_state};
use crate::pipeline::watch::{ReconcileOutcome, WatchEvent};
use crate::tui::probe::Severity;
use crate::tui::projects::ProjectRef;
use crossterm::event::{KeyCode, KeyModifiers};

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
fn drain_events_is_noop_in_poll_mode() {
    let mut state = make_poll_state();
    state.drain_events();
    assert!(state.log.as_slice().is_empty());
}

#[test]
fn handle_key_switches_tabs() {
    let mut state = make_poll_state();
    state.handle_key(KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Repos);
    state.handle_key(KeyCode::Char('2'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Live);
    state.handle_key(KeyCode::Char('3'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Health);
    state.handle_key(KeyCode::Char('4'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Trust);
    state.handle_key(KeyCode::Char('5'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Explain);
    assert!(
        state.explain_preview.is_some(),
        "entering the Explain tab should load the inline preview"
    );
    state.handle_key(KeyCode::Char('6'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Actions);
    state.handle_key(KeyCode::Char('7'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Mcp);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Repos);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Live);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Health);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Trust);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Explain);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Actions);
    state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Mcp);
    state.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Repos);
}

#[test]
fn arrow_keys_cycle_tabs_in_both_directions() {
    let mut state = make_poll_state();
    assert_eq!(state.active_tab, ActiveTab::Live);
    state.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Health);
    state.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Trust);
    state.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Health);
    state.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Live);
    // Wrap around backwards: Live -> Repos.
    state.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Repos);
    // BackTab (Shift-Tab) shares the backward path.
    state.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Mcp);
}

#[test]
fn repos_tab_reuses_recent_project_cache() {
    let mut state = make_poll_state();
    state.explore_projects = vec![ProjectRef {
        id: "cached".to_string(),
        name: "cached".to_string(),
        root: std::path::PathBuf::from("/tmp/cached"),
        health: "ready".to_string(),
        watch: "off".to_string(),
        lock: "free".to_string(),
        integration: "absent".to_string(),
        last_opened_at: None,
    }];
    state.explore_projects_loaded_at = Some(std::time::Instant::now());

    state.set_tab(ActiveTab::Repos);

    assert_eq!(state.explore_projects.len(), 1);
    assert_eq!(state.explore_projects[0].id, "cached");
}

#[test]
fn repos_enter_sets_switch_intent_without_mutating_repo_root() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let (home, _guard) = isolated_home();
    let current = home.path().join("current");
    let next = home.path().join("next");
    std::fs::create_dir_all(&current).unwrap();
    std::fs::create_dir_all(&next).unwrap();
    std::fs::create_dir_all(current.join(".synrepo")).unwrap();
    std::fs::create_dir_all(next.join(".synrepo")).unwrap();
    crate::registry::record_project(&current).unwrap();
    let next_entry = crate::registry::record_project(&next).unwrap();
    let mut state = AppState::new_poll(
        &current,
        crate::tui::theme::Theme::plain(),
        crate::bootstrap::runtime_probe::AgentIntegration::Absent,
    );

    state.set_tab(ActiveTab::Repos);
    state.explore_selected = state
        .explore_projects
        .iter()
        .position(|project| project.id == next_entry.id)
        .unwrap();
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(state.repo_root, current);
    assert_eq!(
        state.exit_intent(),
        DashboardExit::SwitchProject(next.canonicalize().unwrap())
    );
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

#[test]
fn page_keys_use_recorded_live_visible_rows() {
    let mut state = make_poll_state();
    state.live_visible_rows = 6;

    state.handle_key(KeyCode::PageUp, KeyModifiers::NONE);

    assert_eq!(state.scroll_offset, 4);
    assert!(!state.follow_mode);
}
