use super::super::*;
use super::support::{isolated_home, make_live_state, make_poll_state, make_ready_poll_state};
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::tui::actions::{stop_watch, ActionContext, ActionOutcome};
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyModifiers};
use std::fs;

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

fn write_index_sensitive_drift(repo: &std::path::Path) {
    let synrepo_dir = crate::config::Config::synrepo_dir(repo);
    let updated = crate::config::Config {
        roots: vec!["src".to_string()],
        ..crate::config::Config::load(repo).unwrap()
    };
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn demo() {}\n").unwrap();
    fs::write(
        synrepo_dir.join("config.toml"),
        toml::to_string_pretty(&updated).unwrap(),
    )
    .unwrap();
}

fn store_guidance(state: &AppState) -> Vec<String> {
    state
        .snapshot
        .diagnostics
        .as_ref()
        .map(|diag| diag.store_guidance.clone())
        .unwrap_or_default()
}

#[test]
fn pressing_r_does_not_apply_compatibility_actions() {
    let (_tempdir, mut state) = make_ready_poll_state();
    write_index_sensitive_drift(&state.repo_root);
    state.refresh_now();
    let before = store_guidance(&state);
    assert!(
        before.iter().any(|line| line.contains("needs rebuild")),
        "fixture should start stale: {before:?}"
    );

    let consumed = state.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
    assert!(consumed, "'r' should consume refresh");

    let after = store_guidance(&state);
    assert!(
        after.iter().any(|line| line.contains("needs rebuild")),
        "'r' must remain read-only and leave compatibility guidance: {after:?}"
    );
}

#[test]
fn pressing_u_opens_compatibility_confirmation() {
    let (_tempdir, mut state) = make_ready_poll_state();
    let consumed = state.handle_key(KeyCode::Char('U'), KeyModifiers::NONE);
    assert!(consumed, "'U' should consume the key event");
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::ApplyCompatibility)
    );
    let toast = state
        .active_toast()
        .expect("compatibility confirm should set a toast");
    assert!(toast.contains("compatibility apply"));
}

#[test]
fn quick_confirm_esc_cancels_without_exit() {
    let (_tempdir, mut state) = make_ready_poll_state();
    state.handle_key(KeyCode::Char('U'), KeyModifiers::NONE);

    assert!(state.handle_key(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(state.pending_quick_confirm, None);
    assert!(!state.should_exit);
}

#[test]
fn quick_confirm_q_exits() {
    let (_tempdir, mut state) = make_ready_poll_state();
    state.handle_key(KeyCode::Char('U'), KeyModifiers::NONE);

    assert!(state.handle_key(KeyCode::Char('q'), KeyModifiers::NONE));

    assert_eq!(state.pending_quick_confirm, None);
    assert!(state.should_exit);
}

#[test]
fn quick_confirm_tab_switches_and_clears_modal() {
    let (_tempdir, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Health);
    state.handle_key(KeyCode::Char('U'), KeyModifiers::NONE);

    assert!(state.handle_key(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(state.pending_quick_confirm, None);
    assert_eq!(state.active_tab, ActiveTab::Actions);
}

#[test]
fn quick_confirm_ctrl_c_exits() {
    let (_tempdir, mut state) = make_ready_poll_state();
    state.handle_key(KeyCode::Char('U'), KeyModifiers::NONE);

    assert!(state.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(state.pending_quick_confirm, None);
    assert!(state.should_exit);
}

#[test]
fn confirming_u_applies_compatibility_and_clears_guidance() {
    let (_tempdir, mut state) = make_ready_poll_state();
    write_index_sensitive_drift(&state.repo_root);
    state.refresh_now();
    assert!(
        store_guidance(&state)
            .iter()
            .any(|line| line.contains("needs rebuild")),
        "fixture should start stale"
    );

    assert!(state.handle_key(KeyCode::Char('U'), KeyModifiers::NONE));
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    let after = store_guidance(&state);
    assert!(
        after.is_empty(),
        "compatibility apply should clear stale guidance: {after:?}"
    );
    let entry = state
        .log
        .as_slice()
        .last()
        .expect("compatibility apply should log");
    assert_eq!(entry.tag, "compatibility");
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
