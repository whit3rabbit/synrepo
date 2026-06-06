//! AppState MCP Sentry telemetry toggle tests.

use super::super::*;
use crossterm::event::{KeyCode, KeyModifiers};

fn ready_state() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
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
    (lock, repo, guard, state)
}

#[test]
fn quick_actions_include_sentry_telemetry_toggle_disabled_by_default() {
    let (_lock, _repo, _guard, state) = ready_state();
    let action = state
        .quick_actions
        .iter()
        .find(|action| action.key == "O")
        .expect("sentry quick action");
    assert_eq!(action.label, "allow MCP Sentry telemetry");
    assert!(action.requires_confirm);
}

#[test]
fn pressing_o_enables_sentry_telemetry_after_confirmation() {
    let (_lock, repo, _guard, mut state) = ready_state();
    assert!(!crate::config::Config::load(repo.path())
        .unwrap()
        .mcp_sentry_telemetry_enabled());

    assert!(state.handle_key(KeyCode::Char('O'), KeyModifiers::NONE));
    assert_eq!(
        state.pending_quick_confirm,
        Some(PendingQuickConfirm::ToggleSentryTelemetry)
    );

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(crate::config::Config::load(repo.path())
        .unwrap()
        .mcp_sentry_telemetry_enabled());
    let entry = state.log.as_slice().last().expect("sentry log entry");
    assert_eq!(entry.tag, "sentry");
    assert!(entry.message.contains("allowed"));
}

#[test]
fn pressing_o_disables_sentry_telemetry_without_confirmation() {
    let (_lock, repo, _guard, mut state) = ready_state();
    let path = repo.path().join(".synrepo/config.toml");
    let mut config = crate::config::Config::load(repo.path()).unwrap();
    config.mcp_sentry_telemetry = Some(true);
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
    state.refresh_now();

    let action = state
        .quick_actions
        .iter()
        .find(|action| action.key == "O")
        .expect("sentry quick action");
    assert_eq!(action.label, "disable MCP Sentry telemetry");
    assert!(!action.requires_confirm);

    assert!(state.handle_key(KeyCode::Char('O'), KeyModifiers::NONE));
    assert!(state.pending_quick_confirm.is_none());
    assert!(!crate::config::Config::load(repo.path())
        .unwrap()
        .mcp_sentry_telemetry_enabled());
    let entry = state.log.as_slice().last().expect("sentry log entry");
    assert_eq!(entry.tag, "sentry");
    assert!(entry.message.contains("disabled"));
}
