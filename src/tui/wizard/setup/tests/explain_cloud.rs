//! Explain wizard: skip + cloud provider paths.
//!
//! `EXPLAIN_ROWS` is
//! `[Skip, Anthropic, OpenAI, Gemini, OpenRouter, Zai, Minimax, Local]` at
//! index time; these tests pin positions for Skip (0) and Anthropic (1).

use crossterm::event::KeyCode;

use super::support::{drive_to_explain, press, support_with_saved_anthropic, EnvGuard};
use crate::config::Mode;
use crate::tui::wizard::setup::explain::{
    CloudCredentialSource, CloudProvider, ExplainChoice, ExplainRow, EXPLAIN_ROWS,
};
use crate::tui::wizard::setup::state::{SetupStep, SetupWizardState};

#[test]
fn explain_skip_confirms_with_no_choice() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    // First row is Skip; Enter commits.
    assert_eq!(EXPLAIN_ROWS[0], ExplainRow::Skip);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Enter);
    let plan = s.finalize().expect("plan");
    assert!(plan.explain.is_none());
}

#[test]
fn explain_cloud_anthropic_without_detected_key_prompts_for_entry() {
    let _env = EnvGuard::new();
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    press(&mut s, KeyCode::Down); // Skip → Anthropic (index 1)
    assert_eq!(EXPLAIN_ROWS[1], ExplainRow::Cloud(CloudProvider::Anthropic));
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::EditCloudApiKey);
    for ch in "sk-entered".chars() {
        press(&mut s, KeyCode::Char(ch));
    }
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    press(&mut s, KeyCode::Enter); // review → confirm
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Enter);
    let plan = s.finalize().expect("plan");
    assert_eq!(
        plan.explain,
        Some(ExplainChoice::Cloud {
            provider: CloudProvider::Anthropic,
            credential_source: CloudCredentialSource::EnteredGlobal,
            api_key: Some("sk-entered".to_string()),
        })
    );
}

#[test]
fn explain_cloud_anthropic_with_env_key_skips_key_entry() {
    let env = EnvGuard::new();
    env.set("ANTHROPIC_API_KEY", "sk-env");

    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    press(&mut s, KeyCode::Down); // Skip → Anthropic
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    press(&mut s, KeyCode::Enter);
    press(&mut s, KeyCode::Enter);

    let plan = s.finalize().expect("plan");
    assert_eq!(
        plan.explain,
        Some(ExplainChoice::Cloud {
            provider: CloudProvider::Anthropic,
            credential_source: CloudCredentialSource::Env,
            api_key: None,
        })
    );
}

#[test]
fn explain_cloud_anthropic_with_saved_global_key_skips_key_entry() {
    let _env = EnvGuard::new();
    let mut s =
        SetupWizardState::with_explain_support(Mode::Auto, vec![], support_with_saved_anthropic());
    drive_to_explain(&mut s);
    press(&mut s, KeyCode::Down); // Skip → Anthropic
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    press(&mut s, KeyCode::Enter);
    press(&mut s, KeyCode::Enter);

    let plan = s.finalize().expect("plan");
    assert_eq!(
        plan.explain,
        Some(ExplainChoice::Cloud {
            provider: CloudProvider::Anthropic,
            credential_source: CloudCredentialSource::SavedGlobal,
            api_key: None,
        })
    );
}

#[test]
fn explain_cloud_key_entry_escape_returns_to_selector_without_cancel() {
    let _env = EnvGuard::new();
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    press(&mut s, KeyCode::Down); // Skip → Anthropic
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::EditCloudApiKey);
    press(&mut s, KeyCode::Esc);
    assert_eq!(s.step, SetupStep::SelectExplain);
    assert!(!s.cancelled);
    assert!(s.explain.is_none());
}

#[test]
fn explain_cloud_key_entry_empty_input_refuses_enter() {
    let _env = EnvGuard::new();
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    press(&mut s, KeyCode::Down); // Skip → Anthropic
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::EditCloudApiKey);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::EditCloudApiKey);
    assert!(s.explain.is_none());
}

#[test]
fn review_explain_plan_b_clears_choice_and_returns_to_selector() {
    let mut s =
        SetupWizardState::with_explain_support(Mode::Auto, vec![], support_with_saved_anthropic());
    drive_to_explain(&mut s);
    press(&mut s, KeyCode::Down); // Skip → Anthropic
    press(&mut s, KeyCode::Enter); // commit → review
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    assert!(s.explain.is_some());
    press(&mut s, KeyCode::Char('b'));
    assert_eq!(s.step, SetupStep::SelectExplain);
    assert!(
        s.explain.is_none(),
        "backing out of the review screen must clear the pending choice",
    );
}
