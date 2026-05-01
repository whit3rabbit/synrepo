use super::super::*;
use super::support::make_poll_state;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn pressing_e_launches_explain_setup_from_any_tab() {
    let mut state = make_poll_state();
    state.set_tab(ActiveTab::Health);

    let consumed = state.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);

    assert!(consumed, "'e' should consume the key event");
    assert!(state.should_exit);
    assert!(state.launch_explain_setup);
    assert_eq!(state.exit_intent(), DashboardExit::LaunchExplainSetup);
}

#[test]
fn explain_tab_s_still_launches_explain_setup() {
    let mut state = make_poll_state();
    state.set_tab(ActiveTab::Explain);

    let consumed = state.handle_key(KeyCode::Char('s'), KeyModifiers::NONE);

    assert!(consumed, "Explain-tab 's' should stay as a setup alias");
    assert!(state.should_exit);
    assert!(state.launch_explain_setup);
    assert_eq!(state.exit_intent(), DashboardExit::LaunchExplainSetup);
}

#[test]
fn quick_actions_include_configure_explain() {
    let state = make_poll_state();

    assert!(
        state
            .quick_actions
            .iter()
            .any(|action| action.key == "e" && action.label == "configure explain"),
        "dashboard quick actions should advertise explain setup"
    );
}
