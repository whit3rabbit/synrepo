//! Explain wizard: local-preset path (Ollama / llama.cpp / Custom).
//!
//! The tests locate the Local row dynamically so adding more cloud rows
//! in the middle of `EXPLAIN_ROWS` is safe.

use crossterm::event::{KeyCode, KeyModifiers};

use super::support::{drive_to_explain, press};
use crate::config::Mode;
use crate::tui::wizard::setup::explain::{ExplainChoice, ExplainRow, LocalPreset, EXPLAIN_ROWS};
use crate::tui::wizard::setup::state::{SetupStep, SetupWizardState};

#[test]
fn explain_local_preset_ollama_default_endpoint() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    // Walk to the Local row (last in EXPLAIN_ROWS).
    let local_index = EXPLAIN_ROWS
        .iter()
        .position(|r| matches!(r, ExplainRow::Local))
        .expect("Local row present");
    for _ in 0..local_index {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectLocalPreset);
    // Ollama is at index 0 in LOCAL_PRESETS; Enter accepts with its default endpoint.
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::EditLocalEndpoint);
    assert_eq!(
        s.endpoint_input.value(),
        LocalPreset::Ollama.default_endpoint()
    );
    press(&mut s, KeyCode::Enter); // accept endpoint unchanged → review
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    press(&mut s, KeyCode::Enter); // review → confirm
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Enter);
    let plan = s.finalize().expect("plan");
    assert_eq!(
        plan.explain,
        Some(ExplainChoice::Local {
            preset: LocalPreset::Ollama,
            endpoint: LocalPreset::Ollama.default_endpoint().to_string(),
        })
    );
}

#[test]
fn explain_local_custom_endpoint_is_editable() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    let local_index = EXPLAIN_ROWS
        .iter()
        .position(|r| matches!(r, ExplainRow::Local))
        .expect("Local row present");
    for _ in 0..local_index {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter); // into preset list
                                   // Move to Custom (last preset).
    for _ in 0..4 {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter); // into endpoint editor
    assert_eq!(s.step, SetupStep::EditLocalEndpoint);
    // Clear the pre-filled default and type a fresh URL.
    s.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
    for ch in "http://gpu-box:9000/v1/chat/completions".chars() {
        press(&mut s, KeyCode::Char(ch));
    }
    press(&mut s, KeyCode::Enter); // endpoint → review
    assert_eq!(s.step, SetupStep::ReviewExplainPlan);
    press(&mut s, KeyCode::Enter); // review → confirm
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Enter); // confirm → complete
    let plan = s.finalize().expect("plan");
    assert_eq!(
        plan.explain,
        Some(ExplainChoice::Local {
            preset: LocalPreset::Custom,
            endpoint: "http://gpu-box:9000/v1/chat/completions".to_string(),
        })
    );
}

#[test]
fn explain_endpoint_esc_returns_to_preset_without_cancel() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    let local_index = EXPLAIN_ROWS
        .iter()
        .position(|r| matches!(r, ExplainRow::Local))
        .expect("Local row present");
    for _ in 0..local_index {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter); // into preset list
    press(&mut s, KeyCode::Enter); // into endpoint editor
    assert_eq!(s.step, SetupStep::EditLocalEndpoint);
    press(&mut s, KeyCode::Esc);
    assert_eq!(s.step, SetupStep::SelectLocalPreset);
    assert!(!s.cancelled, "Esc from endpoint editor must not cancel");
    assert!(s.explain.is_none(), "no choice committed yet");
}

#[test]
fn explain_endpoint_empty_input_refuses_enter() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    let local_index = EXPLAIN_ROWS
        .iter()
        .position(|r| matches!(r, ExplainRow::Local))
        .expect("Local row present");
    for _ in 0..local_index {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter); // into preset list
    press(&mut s, KeyCode::Enter); // into endpoint editor
    s.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL); // clear
    press(&mut s, KeyCode::Enter);
    // Still on editor — empty endpoint is a silent no-op, not a transition.
    assert_eq!(s.step, SetupStep::EditLocalEndpoint);
    assert!(s.explain.is_none());
}

#[test]
fn explain_preset_switch_reseeds_endpoint_default() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);
    let local_index = EXPLAIN_ROWS
        .iter()
        .position(|r| matches!(r, ExplainRow::Local))
        .expect("Local row present");
    for _ in 0..local_index {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter); // into preset list
                                   // Accept Ollama first.
    press(&mut s, KeyCode::Enter);
    assert_eq!(
        s.endpoint_input.value(),
        LocalPreset::Ollama.default_endpoint()
    );
    press(&mut s, KeyCode::Esc); // back to preset list
    assert_eq!(s.step, SetupStep::SelectLocalPreset);
    // Select llama.cpp (index 1).
    press(&mut s, KeyCode::Down);
    press(&mut s, KeyCode::Enter);
    assert_eq!(
        s.endpoint_input.value(),
        LocalPreset::LlamaCpp.default_endpoint(),
        "switching preset must reseed the text buffer with the new default",
    );
}
