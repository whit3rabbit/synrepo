//! Navigation, cancellation, target selection, and back-button tests.

use crossterm::event::{KeyCode, KeyModifiers};

use super::support::{press, support_with_saved_anthropic};
use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::config::Mode;
use crate::tui::wizard::setup::state::{SetupStep, SetupWizardState, WIZARD_TARGETS};

#[test]
fn happy_path_default_auto_claude_target() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);
    assert_eq!(s.step, SetupStep::Splash);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectMode);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectTarget);
    assert_eq!(s.mode, Mode::Auto);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::ExplainExplain);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectExplain);
    press(&mut s, KeyCode::Enter);
    // Default explain cursor is 0 (Skip) — goes straight to Confirm.
    assert_eq!(s.step, SetupStep::Confirm);
    assert_eq!(s.target, Some(AgentTargetKind::Claude));
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::Complete);
    let plan = s.finalize().expect("plan");
    assert_eq!(plan.mode, Mode::Auto);
    assert_eq!(plan.target, Some(AgentTargetKind::Claude));
    assert!(plan.reconcile_after);
}

#[test]
fn select_curated_and_skip_target() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Down);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.mode, Mode::Curated);
    for _ in 0..WIZARD_TARGETS.len() {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.target, None);
    assert_eq!(s.step, SetupStep::ExplainExplain);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectExplain);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Enter);
    let plan = s.finalize().expect("plan");
    assert_eq!(plan.mode, Mode::Curated);
    assert_eq!(plan.target, None);
}

#[test]
fn splash_enter_advances_to_mode() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    assert_eq!(s.step, SetupStep::Splash);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectMode);
    assert!(!s.cancelled);
}

#[test]
fn esc_at_splash_cancels_without_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Esc);
    assert!(s.cancelled);
    assert_eq!(s.step, SetupStep::Complete);
    assert!(s.finalize().is_none());
}

#[test]
fn q_at_splash_cancels_without_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Char('q'));
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn ctrl_c_at_splash_cancels() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn esc_at_mode_step_cancels_with_no_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Esc);
    assert!(s.cancelled);
    assert_eq!(s.step, SetupStep::Complete);
    assert!(s.finalize().is_none());
}

#[test]
fn esc_at_target_step_cancels_with_no_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Enter);
    press(&mut s, KeyCode::Esc);
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn b_at_confirm_after_skip_goes_back_to_explain_selection() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // splash → mode
    press(&mut s, KeyCode::Enter); // mode → target
    press(&mut s, KeyCode::Enter); // target → explain
    press(&mut s, KeyCode::Enter); // explain → explain
    assert_eq!(s.step, SetupStep::SelectExplain);
    press(&mut s, KeyCode::Enter); // Skip committed; jumps to confirm
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Char('b'));
    // With explain skipped, back jumps over the (unvisited) review step
    // and lands on the provider selector.
    assert_eq!(s.step, SetupStep::SelectExplain);
    assert!(!s.cancelled);
}

#[test]
fn b_at_confirm_after_provider_goes_back_to_review() {
    let mut s =
        SetupWizardState::with_explain_support(Mode::Auto, vec![], support_with_saved_anthropic());
    press(&mut s, KeyCode::Enter); // splash → mode
    press(&mut s, KeyCode::Enter); // mode → target
    press(&mut s, KeyCode::Enter); // target → explain
    press(&mut s, KeyCode::Enter); // explain → explain
    press(&mut s, KeyCode::Down); // Skip → Anthropic
    press(&mut s, KeyCode::Enter); // commit Anthropic → review
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    press(&mut s, KeyCode::Enter); // review → confirm
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Char('b'));
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    assert!(!s.cancelled);
}

#[test]
fn ctrl_c_at_confirm_cancels() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // splash → mode
    press(&mut s, KeyCode::Enter); // mode → target
    press(&mut s, KeyCode::Enter); // target → explain
    press(&mut s, KeyCode::Enter); // explain → explain
    press(&mut s, KeyCode::Enter); // Skip → confirm
    s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn detected_target_preselects_cursor_when_available() {
    let s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Codex]);
    assert_eq!(s.target_cursor, 2);
}

#[test]
fn detected_target_absent_from_roster_falls_back_to_zero() {
    let s = SetupWizardState::new(Mode::Curated, vec![]);
    assert_eq!(s.target_cursor, 0);
}

#[test]
fn up_at_top_does_not_underflow() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Up);
    assert_eq!(s.mode_cursor, 0);
}

#[test]
fn down_at_bottom_does_not_overflow() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    for _ in 0..10 {
        press(&mut s, KeyCode::Down);
    }
    assert_eq!(s.mode_cursor, 1);
}

#[test]
fn explain_explain_b_goes_back_to_select_target() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // splash → mode
    press(&mut s, KeyCode::Enter); // mode → target
    press(&mut s, KeyCode::Enter); // target → explain
    assert_eq!(s.step, SetupStep::ExplainExplain);
    press(&mut s, KeyCode::Char('b'));
    assert_eq!(s.step, SetupStep::SelectTarget);
    assert!(!s.cancelled);
}
