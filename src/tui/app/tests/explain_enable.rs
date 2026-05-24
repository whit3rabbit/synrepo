use super::super::*;
use super::support::{force_explain_status, make_ready_poll_state};
use crate::pipeline::explain::ExplainStatus;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn disabled_explain_run_opens_enable_prompt() {
    let (_repo, mut state) = make_ready_poll_state();
    force_explain_status(&mut state, ExplainStatus::Disabled);
    state.set_tab(ActiveTab::Explain);

    let consumed = state.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);

    assert!(consumed);
    assert!(matches!(
        state.confirm_enable_explain.as_ref(),
        Some(ConfirmEnableExplainState {
            mode: ExplainMode::AllStale
        })
    ));
    assert!(state.pending_explain.is_empty());
    assert!(state.confirm_stop_watch.is_none());
    assert!(!state.should_exit);
}

#[test]
fn enable_prompt_y_launches_explain_setup() {
    let (_repo, mut state) = make_ready_poll_state();
    force_explain_status(&mut state, ExplainStatus::Disabled);
    state.set_tab(ActiveTab::Explain);
    assert!(state.handle_key(KeyCode::Char('a'), KeyModifiers::NONE));

    let consumed = state.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);

    assert!(consumed);
    assert!(state.confirm_enable_explain.is_none());
    assert!(state.pending_explain.is_empty());
    assert!(state.should_exit);
    assert!(state.launch_explain_setup);
    assert_eq!(state.exit_intent(), DashboardExit::LaunchExplainSetup);
}

#[test]
fn enable_prompt_n_cancels_without_queueing() {
    let (_repo, mut state) = make_ready_poll_state();
    force_explain_status(&mut state, ExplainStatus::Disabled);
    state.set_tab(ActiveTab::Explain);
    assert!(state.handle_key(KeyCode::Char('a'), KeyModifiers::NONE));

    let consumed = state.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);

    assert!(consumed);
    assert!(state.confirm_enable_explain.is_none());
    assert!(state.pending_explain.is_empty());
    assert!(!state.should_exit);
    assert!(!state.launch_explain_setup);
}
