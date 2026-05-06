//! Setup wizard state machine logic (impl blocks).
//! Type definitions live in the sibling `state_types` module.

use crossterm::event::{KeyCode, KeyModifiers};

use super::explain::{ExplainWizardSupport, TextInputField, LOCAL_PRESETS};
pub use super::state_types::{
    SetupPlan, SetupStep, SetupWizardOutcome, SetupWizardState, WIZARD_TARGETS,
};
use crate::config::Mode;

mod transitions;

impl SetupWizardState {
    /// Build a fresh state. `default_mode` seeds the mode cursor; a caller
    /// that detected concept directories passes `Mode::Curated`.
    /// `detected_targets` seeds the target cursor to the first detected
    /// target, falling back to position 0 (the first target in
    /// [`WIZARD_TARGETS`]).
    pub fn new(
        default_mode: Mode,
        detected_targets: Vec<crate::bootstrap::runtime_probe::AgentTargetKind>,
    ) -> Self {
        Self::with_explain_support(
            default_mode,
            detected_targets,
            ExplainWizardSupport::default(),
        )
    }

    /// Same as [`SetupWizardState::new`], but seeds the explain flow with
    /// observed env/global config support from the caller.
    pub fn with_explain_support(
        default_mode: Mode,
        detected_targets: Vec<crate::bootstrap::runtime_probe::AgentTargetKind>,
        explain_support: ExplainWizardSupport,
    ) -> Self {
        let mode_cursor = match default_mode {
            Mode::Auto => 0,
            Mode::Curated => 1,
        };
        let target_cursor = detected_targets
            .first()
            .and_then(|t| WIZARD_TARGETS.iter().position(|wt| wt == t))
            .unwrap_or(0);
        let local_preset = explain_support.local_preset();
        let local_preset_cursor = LOCAL_PRESETS
            .iter()
            .position(|preset| *preset == local_preset)
            .unwrap_or(0);
        let endpoint_seed = explain_support
            .local_endpoint()
            .unwrap_or(local_preset.default_endpoint());
        Self {
            step: SetupStep::Splash,
            mode_cursor,
            target_cursor,
            embeddings_cursor: 0,
            explain_cursor: 0,
            local_preset_cursor,
            mode: default_mode,
            target: None,
            enable_embeddings: false,
            explain: None,
            endpoint_input: TextInputField::with_value(endpoint_seed),
            api_key_input: TextInputField::with_value(""),
            local_preset,
            pending_cloud_provider: None,
            explain_support,
            detected_targets,
            cancelled: false,
        }
    }

    /// Build a state positioned directly at `SelectExplain`, used by
    /// `synrepo setup --explain` to run only the explain sub-flow. The
    /// plan's `mode`/`target`/`enable_embeddings` fields are placeholders;
    /// the caller must only consume `plan.explain`.
    pub fn explain_only() -> Self {
        let mut s = Self::with_explain_support(Mode::Auto, vec![], ExplainWizardSupport::default());
        s.step = SetupStep::SelectExplain;
        s
    }

    /// Same as [`SetupWizardState::explain_only`], but seeds the explain
    /// flow with observed env/global config support from the caller.
    pub fn explain_only_with_support(explain_support: ExplainWizardSupport) -> Self {
        let mut s = Self::with_explain_support(Mode::Auto, vec![], explain_support);
        s.step = SetupStep::SelectExplain;
        s
    }

    /// Handle a key event; returns `true` if the loop should redraw. Pressing
    /// Esc / Ctrl-C / q at any step before `Confirm` cancels the wizard. At
    /// `Confirm`, Esc / b steps back rather than cancelling; Ctrl-C still
    /// cancels.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match self.step {
            SetupStep::Splash => self.handle_splash_key(code, modifiers),
            SetupStep::SelectMode => self.handle_select_mode_key(code, modifiers),
            SetupStep::SelectTarget => self.handle_select_target_key(code, modifiers),
            SetupStep::SelectEmbeddings => self.handle_select_embeddings_key(code, modifiers),
            SetupStep::ExplainExplain => self.handle_explain_explain_key(code, modifiers),
            SetupStep::SelectExplain => self.handle_select_explain_key(code, modifiers),
            SetupStep::EditCloudApiKey => self.handle_edit_cloud_api_key_key(code, modifiers),
            SetupStep::SelectLocalPreset => self.handle_select_local_preset_key(code, modifiers),
            SetupStep::EditLocalEndpoint => self.handle_edit_local_endpoint_key(code, modifiers),
            SetupStep::ReviewExplainPlan => self.handle_review_explain_plan_key(code, modifiers),
            SetupStep::Confirm => self.handle_confirm_key(code, modifiers),
            SetupStep::Complete => false,
        }
    }

    /// Cancel the wizard and transition to Complete. Shared by every
    /// non-text-input step's Esc / q / Ctrl-C handling.
    pub(super) fn cancel_to_complete(&mut self) -> bool {
        self.cancelled = true;
        self.step = SetupStep::Complete;
        true
    }

    pub(super) fn is_quit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
        matches!(code, KeyCode::Esc | KeyCode::Char('q'))
            || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL))
    }

    /// If the state machine completed without cancelling, produce the plan.
    pub fn finalize(&self) -> Option<SetupPlan> {
        if self.cancelled || self.step != SetupStep::Complete {
            return None;
        }
        Some(SetupPlan {
            mode: self.mode,
            target: self.target,
            enable_embeddings: self.enable_embeddings,
            explain: self.explain.clone(),
            reconcile_after: true,
        })
    }
}
