//! Optional embeddings setup step tests.

use crossterm::event::KeyCode;

use super::support::press;
use crate::config::Mode;
use crate::tui::wizard::setup::state::{EmbeddingSetupChoice, SetupStep, SetupWizardState};

fn drive_to_embeddings(s: &mut SetupWizardState) {
    press(s, KeyCode::Enter); // splash -> mode
    press(s, KeyCode::Enter); // mode -> target
    press(s, KeyCode::Enter); // target -> actions
    assert_eq!(s.step, SetupStep::SelectActions);
    press(s, KeyCode::Enter); // actions -> embeddings
    assert_eq!(s.step, SetupStep::SelectEmbeddings);
}

#[test]
fn embeddings_default_skips_to_explain() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_embeddings(&mut s);

    press(&mut s, KeyCode::Enter);

    assert_eq!(s.embedding_setup, EmbeddingSetupChoice::Disabled);
    assert_eq!(s.step, SetupStep::ExplainExplain);
}

#[test]
fn embeddings_enable_is_recorded_in_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_embeddings(&mut s);

    press(&mut s, KeyCode::Down);
    press(&mut s, KeyCode::Enter);
    press(&mut s, KeyCode::Enter); // explain explainer -> selector
    press(&mut s, KeyCode::Enter); // skip explain -> confirm
    press(&mut s, KeyCode::Enter); // apply

    let plan = s.finalize().expect("plan");
    assert_eq!(plan.embedding_setup, EmbeddingSetupChoice::Onnx);
    assert!(plan.explain.is_none());
}

#[test]
fn embeddings_b_returns_to_actions() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_embeddings(&mut s);

    press(&mut s, KeyCode::Char('b'));

    assert_eq!(s.step, SetupStep::SelectActions);
    assert!(!s.cancelled);
}

#[test]
fn embeddings_escape_cancels_without_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_embeddings(&mut s);

    press(&mut s, KeyCode::Esc);

    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}
