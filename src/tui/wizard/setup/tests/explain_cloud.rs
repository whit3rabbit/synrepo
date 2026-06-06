//! Explain wizard: skip + cloud provider paths.
//!
//! `EXPLAIN_ROWS` is
//! `[Skip, Anthropic, OpenAI, Gemini, OpenRouter, Zai, Minimax, Local]` at
//! index time; these tests pin positions for Skip (0) and Anthropic (1).

use std::path::Path;

use crossterm::event::KeyCode;

use super::support::{drive_to_explain, press, support_with_saved_anthropic, EnvGuard};
use crate::config::Mode;
use crate::tui::wizard::setup::explain::{
    CloudCredentialSource, CloudProvider, ExplainChoice, ExplainRow, ExplainWizardSupport,
    EXPLAIN_ROWS,
};
use crate::tui::wizard::setup::state::{SetupStep, SetupWizardState};

fn explain_row_index(row: ExplainRow) -> usize {
    EXPLAIN_ROWS
        .iter()
        .position(|candidate| *candidate == row)
        .expect("row present")
}

fn write_repo_config(repo_root: &Path, body: &str) {
    let synrepo_dir = repo_root.join(".synrepo");
    std::fs::create_dir_all(&synrepo_dir).expect("mkdir .synrepo");
    std::fs::write(synrepo_dir.join("config.toml"), body).expect("write config");
}

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
fn full_setup_still_starts_explain_on_skip() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    drive_to_explain(&mut s);

    assert_eq!(s.explain_cursor, explain_row_index(ExplainRow::Skip));
}

#[test]
fn explain_only_preselects_repo_openai_provider() {
    let repo = tempfile::tempdir().expect("tempdir");
    write_repo_config(
        repo.path(),
        r#"
            [explain]
            enabled = true
            provider = "openai"
        "#,
    );

    let s = SetupWizardState::explain_only_with_support(ExplainWizardSupport::detect_for_repo(
        repo.path(),
    ));

    assert_eq!(s.step, SetupStep::SelectExplain);
    assert_eq!(
        s.explain_cursor,
        explain_row_index(ExplainRow::Cloud(CloudProvider::OpenAi))
    );
}

#[test]
fn explain_only_preselects_repo_local_provider() {
    let repo = tempfile::tempdir().expect("tempdir");
    write_repo_config(
        repo.path(),
        r#"
            [explain]
            enabled = true
            provider = "local"
        "#,
    );

    let s = SetupWizardState::explain_only_with_support(ExplainWizardSupport::detect_for_repo(
        repo.path(),
    ));

    assert_eq!(s.step, SetupStep::SelectExplain);
    assert_eq!(s.explain_cursor, explain_row_index(ExplainRow::Local));
}

#[test]
fn explain_only_unknown_missing_none_or_unreadable_provider_stays_on_skip() {
    let cases = [
        ("absent config", None),
        ("missing provider", Some("[explain]\nenabled = true\n")),
        ("provider none", Some("[explain]\nprovider = \"none\"\n")),
        (
            "unknown provider",
            Some("[explain]\nprovider = \"bogus\"\n"),
        ),
        ("invalid toml", Some("[explain\nprovider = \"openai\"\n")),
    ];

    for (label, config) in cases {
        let repo = tempfile::tempdir().expect("tempdir");
        if let Some(config) = config {
            write_repo_config(repo.path(), config);
        }
        let s = SetupWizardState::explain_only_with_support(ExplainWizardSupport::detect_for_repo(
            repo.path(),
        ));

        assert_eq!(
            s.explain_cursor,
            explain_row_index(ExplainRow::Skip),
            "{label} should default to Skip"
        );
    }
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
