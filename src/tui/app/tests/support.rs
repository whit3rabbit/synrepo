//! Shared fixture helpers for AppState tests.

use super::super::*;
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::watch::WatchEvent;
use crate::tui::theme::Theme;

pub(super) fn make_poll_state() -> AppState {
    let tempdir = tempfile::tempdir().unwrap();
    AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent)
}

pub(super) fn make_live_state() -> (AppState, crossbeam_channel::Sender<WatchEvent>) {
    let tempdir = tempfile::tempdir().unwrap();
    let (tx, rx) = crossbeam_channel::bounded::<WatchEvent>(16);
    let state = AppState::new_live(tempdir.path(), Theme::plain(), AgentIntegration::Absent, rx);
    // Keep the tempdir alive via Box::leak so the caller doesn't have to manage it.
    // Safe: these are short-lived unit tests.
    std::mem::forget(tempdir);
    (state, tx)
}

pub(super) fn make_ready_poll_state() -> (tempfile::TempDir, AppState) {
    let tempdir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");
    let state = AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
    (tempdir, state)
}

pub(super) fn isolated_home() -> (tempfile::TempDir, crate::config::test_home::HomeEnvGuard) {
    let home = tempfile::tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard)
}
