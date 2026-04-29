//! AppState materialize-on-tick tests. Cover the auto-fire path that fires
//! once per session and the manual `M` key path that bypasses the
//! attempted-auto guard.

use super::super::*;
use super::support::{isolated_home, make_ready_poll_state};
use crate::tui::materializer::MaterializeState;
use crossterm::event::{KeyCode, KeyModifiers};
use std::time::Instant;

/// Drain the supervisor by ticking until it reports a non-running state or
/// the deadline expires. Returns the final `MaterializeState`.
fn wait_for_materialize_finish(state: &mut AppState) -> MaterializeState {
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    loop {
        state.tick();
        let s = state.materialize_state.clone();
        if !matches!(s, MaterializeState::Running { .. }) {
            return s;
        }
        if Instant::now() >= deadline {
            panic!("materializer did not finish within 30s");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn tick_auto_fires_materialize_when_graph_missing() {
    let _guard = crate::test_support::global_test_lock("tui-app-materialize");
    let (_home, _home_guard) = isolated_home();
    let (repo, mut state) = make_ready_poll_state();
    // Drop the graph store so the snapshot reports `graph_stats = None`.
    let graph_dir = repo.path().join(".synrepo/graph");
    std::fs::remove_dir_all(&graph_dir).expect("remove graph dir");

    // Force a snapshot rebuild so the state observes the missing graph
    // store; otherwise `tick()` would still see the cached snapshot.
    state.refresh_now();
    assert!(
        state.snapshot.graph_stats.is_none(),
        "snapshot should report missing graph after rmdir"
    );
    assert!(!state.materializer.auto_was_attempted());

    state.tick();

    assert!(
        state.materializer.auto_was_attempted(),
        "tick must mark the one-shot auto attempt"
    );
    assert!(
        state
            .log
            .as_slice()
            .iter()
            .any(|entry| entry.tag == "materialize" && entry.message.contains("auto:")),
        "log must record an auto-fire entry, got {:?}",
        state.log.as_slice()
    );

    // Drain to keep the test from leaking a thread.
    let _ = wait_for_materialize_finish(&mut state);
}

#[test]
fn tick_does_not_re_fire_auto_after_first_attempt() {
    let _guard = crate::test_support::global_test_lock("tui-app-materialize");
    let (_home, _home_guard) = isolated_home();
    let (repo, mut state) = make_ready_poll_state();
    let graph_dir = repo.path().join(".synrepo/graph");
    std::fs::remove_dir_all(&graph_dir).expect("remove graph dir");
    state.refresh_now();

    state.tick();
    let _ = wait_for_materialize_finish(&mut state);

    // After the auto-attempt has resolved, even if the snapshot still says
    // graph is missing (e.g. failure), tick() must not start another run.
    let auto_log_count_before = state
        .log
        .as_slice()
        .iter()
        .filter(|e| e.tag == "materialize" && e.message.contains("auto:"))
        .count();
    assert_eq!(
        auto_log_count_before, 1,
        "exactly one auto entry expected after first attempt, got {auto_log_count_before}"
    );

    // Wipe the graph again to simulate the dashboard re-observing
    // `graph_stats = None` post-failure.
    if graph_dir.exists() {
        std::fs::remove_dir_all(&graph_dir).expect("remove graph dir");
    }
    state.refresh_now();
    state.tick();
    state.tick();

    let auto_log_count_after = state
        .log
        .as_slice()
        .iter()
        .filter(|e| e.tag == "materialize" && e.message.contains("auto:"))
        .count();
    assert_eq!(
        auto_log_count_after, 1,
        "auto-attempt must remain one-shot per session"
    );
}

#[test]
fn manual_m_press_dispatches_even_after_auto_attempted() {
    let _guard = crate::test_support::global_test_lock("tui-app-materialize");
    let (_home, _home_guard) = isolated_home();
    let (_repo, mut state) = make_ready_poll_state();

    // Pretend an auto attempt already fired this session so the auto path
    // would be suppressed; the manual key must still work.
    state.materializer.mark_auto_attempted();
    let log_len_before = state.log.as_slice().len();

    let consumed = state.handle_key(KeyCode::Char('M'), KeyModifiers::NONE);
    assert!(consumed, "M must consume the key event");
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::MaterializeGraph)
    );
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        state.log.as_slice().len() > log_len_before,
        "confirmed M must record a log entry"
    );
    let entry = state.log.as_slice().last().unwrap();
    assert_eq!(entry.tag, "materialize");

    // Drain the supervisor so the test does not leak a thread.
    let _ = wait_for_materialize_finish(&mut state);
}

#[test]
fn quick_actions_include_m_when_graph_missing() {
    let _guard = crate::test_support::global_test_lock("tui-app-materialize");
    let (_home, _home_guard) = isolated_home();
    let (repo, mut state) = make_ready_poll_state();
    std::fs::remove_dir_all(repo.path().join(".synrepo/graph")).expect("remove graph dir");
    state.refresh_now();

    let has_m = state
        .quick_actions
        .iter()
        .any(|a| a.key == "M" && a.label.contains("generate graph"));
    assert!(has_m, "missing M quick action: {:?}", state.quick_actions);
}

#[test]
fn quick_actions_omit_m_when_graph_present() {
    let _guard = crate::test_support::global_test_lock("tui-app-materialize");
    let (_home, _home_guard) = isolated_home();
    let (_repo, state) = make_ready_poll_state();
    let has_m = state.quick_actions.iter().any(|a| a.key == "M");
    assert!(!has_m, "stray M quick action: {:?}", state.quick_actions);
}
