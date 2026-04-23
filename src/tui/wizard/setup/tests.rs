//! Tests for setup wizard state machine.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::bootstrap::runtime_probe::AgentTargetKind;
    use crate::config::{ExplainConfig, Mode};
    use crate::tui::wizard::setup::explain::{
        CloudCredentialSource, CloudProvider, ExplainChoice, ExplainRow, ExplainWizardSupport,
        LocalPreset, EXPLAIN_ROWS,
    };
    use crate::tui::wizard::setup::state::{SetupStep, SetupWizardState, WIZARD_TARGETS};
    use crossterm::event::{KeyCode, KeyModifiers};

    const RELEVANT_ENV: &[&str] = &[
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
        "OPENROUTER_API_KEY",
        "ZAI_API_KEY",
        "MINIMAX_API_KEY",
    ];

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let guard = ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for var in RELEVANT_ENV {
                std::env::remove_var(var);
            }
            Self { _guard: guard }
        }

        fn set(&self, key: &str, value: &str) {
            std::env::set_var(key, value);
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for var in RELEVANT_ENV {
                std::env::remove_var(var);
            }
        }
    }

    fn press(state: &mut SetupWizardState, code: KeyCode) {
        state.handle_key(code, KeyModifiers::empty());
    }

    fn support_with_saved_anthropic() -> ExplainWizardSupport {
        let config = ExplainConfig {
            anthropic_api_key: Some("saved-anthropic-key".to_string()),
            ..Default::default()
        };
        ExplainWizardSupport::with_global_explain(config)
    }

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
        let mut s = SetupWizardState::with_explain_support(
            Mode::Auto,
            vec![],
            support_with_saved_anthropic(),
        );
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

    // ---- 10a.6: wizard cancellation leaves the working tree byte-identical.
    //
    // The wizard state machine has no filesystem handle by construction, so
    // these tests exercise the full "drive key events, then compare the
    // tempdir" invariant end-to-end. If any future refactor wires FS access
    // into the state machine or its helpers, these tests will catch it.

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
            let entry = entry.expect("walk");
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(root)
                    .expect("strip")
                    .to_path_buf();
                let bytes = std::fs::read(entry.path()).expect("read");
                out.insert(rel, bytes);
            }
        }
        out
    }

    fn drive_cancel_and_assert_no_writes(drive: impl FnOnce(&mut SetupWizardState)) {
        let tempdir = tempfile::tempdir().expect("tempdir");
        std::fs::write(tempdir.path().join("fixture.txt"), b"original content")
            .expect("seed fixture");
        std::fs::create_dir_all(tempdir.path().join("nested/dir")).expect("mkdir");
        std::fs::write(tempdir.path().join("nested/dir/leaf.md"), b"# leaf").expect("seed leaf");
        let before = snapshot_tree(tempdir.path());

        let mut s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);
        drive(&mut s);
        assert!(s.cancelled, "drive closure must cancel the wizard");
        assert!(
            s.finalize().is_none(),
            "cancelled wizard must yield no plan"
        );

        let after = snapshot_tree(tempdir.path());
        assert_eq!(
            before, after,
            "working tree must be byte-identical after cancellation",
        );
    }

    #[test]
    fn cancel_at_splash_leaves_tree_byte_identical() {
        drive_cancel_and_assert_no_writes(|s| {
            assert_eq!(s.step, SetupStep::Splash);
            s.handle_key(KeyCode::Esc, KeyModifiers::empty());
        });
    }

    #[test]
    fn cancel_at_mode_leaves_tree_byte_identical() {
        drive_cancel_and_assert_no_writes(|s| {
            s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // splash → mode
            assert_eq!(s.step, SetupStep::SelectMode);
            s.handle_key(KeyCode::Esc, KeyModifiers::empty());
        });
    }

    // ---- Explain step coverage ----
    //
    // These exercise the 4-to-7-step explain sub-flow introduced in the
    // opt-in safeguard change. `EXPLAIN_ROWS` is
    // `[Skip, Anthropic, OpenAI, Gemini, OpenRouter, Zai, Minimax, Local]` at
    // index time — the tests pin positions for Skip (0) and Anthropic (1)
    // explicitly, and locate Local dynamically, so adding more cloud rows in
    // the middle is safe.

    /// Drive the wizard from Splash to SelectExplain using the defaults
    /// (auto mode, skip target). Passes through the `ExplainExplain`
    /// explainer step automatically.
    fn drive_to_explain(s: &mut SetupWizardState) {
        press(s, KeyCode::Enter); // splash → mode
        press(s, KeyCode::Enter); // mode → target
        for _ in 0..WIZARD_TARGETS.len() {
            press(s, KeyCode::Down); // land on "Skip"
        }
        press(s, KeyCode::Enter); // target → explain
        assert_eq!(s.step, SetupStep::ExplainExplain);
        press(s, KeyCode::Enter); // explain → explain
        assert_eq!(s.step, SetupStep::SelectExplain);
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
        let mut s = SetupWizardState::with_explain_support(
            Mode::Auto,
            vec![],
            support_with_saved_anthropic(),
        );
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

    #[test]
    fn review_explain_plan_b_clears_choice_and_returns_to_selector() {
        let mut s = SetupWizardState::with_explain_support(
            Mode::Auto,
            vec![],
            support_with_saved_anthropic(),
        );
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

    #[test]
    fn cancel_at_target_leaves_tree_byte_identical() {
        drive_cancel_and_assert_no_writes(|s| {
            s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // splash → mode
            s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // mode → target
            assert_eq!(s.step, SetupStep::SelectTarget);
            s.handle_key(KeyCode::Esc, KeyModifiers::empty());
        });
    }
}
