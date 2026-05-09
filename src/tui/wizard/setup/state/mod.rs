//! Setup wizard state machine logic (impl blocks).
//! Type definitions live in the sibling `state_types` module.

use crossterm::event::{KeyCode, KeyModifiers};

use super::explain::{ExplainWizardSupport, TextInputField, LOCAL_PRESETS};
pub use super::state_types::{
    EmbeddingSetupChoice, SetupActionRow, SetupFlow, SetupPlan, SetupStep, SetupWizardOutcome,
    SetupWizardState, SETUP_ACTION_ROWS, WIZARD_TARGETS,
};
use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::config::Mode;
use crate::tui::wizard::{target_tier, AgentTargetTier};

mod actions;
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
        detected_targets: Vec<AgentTargetKind>,
        explain_support: ExplainWizardSupport,
    ) -> Self {
        Self::with_setup_context(
            default_mode,
            detected_targets,
            AgentIntegration::Absent,
            SetupFlow::Full,
            false,
            explain_support,
        )
    }

    /// Build a state with caller-provided setup context. Full setup starts at
    /// Splash; follow-up setup starts at target selection.
    pub fn with_setup_context(
        default_mode: Mode,
        detected_targets: Vec<AgentTargetKind>,
        current_integration: AgentIntegration,
        flow: SetupFlow,
        root_gitignore_present: bool,
        explain_support: ExplainWizardSupport,
    ) -> Self {
        let mode_cursor = match default_mode {
            Mode::Auto => 0,
            Mode::Curated => 1,
        };
        let seed_target = current_integration
            .target()
            .or_else(|| detected_targets.first().copied());
        let target_cursor = seed_target
            .and_then(|t| WIZARD_TARGETS.iter().position(|wt| *wt == t))
            .unwrap_or(0);
        let target = Some(WIZARD_TARGETS[target_cursor]);
        let (write_agent_shim, register_mcp) =
            default_agent_actions_for(&current_integration, target);
        let local_preset = explain_support.local_preset();
        let local_preset_cursor = LOCAL_PRESETS
            .iter()
            .position(|preset| *preset == local_preset)
            .unwrap_or(0);
        let endpoint_seed = explain_support
            .local_endpoint()
            .unwrap_or(local_preset.default_endpoint());
        Self {
            step: match flow {
                SetupFlow::Full => SetupStep::Splash,
                SetupFlow::FollowUp => SetupStep::SelectTarget,
            },
            mode_cursor,
            target_cursor,
            action_cursor: 0,
            embeddings_cursor: 0,
            explain_cursor: 0,
            local_preset_cursor,
            mode: default_mode,
            target,
            flow,
            current_integration,
            root_gitignore_present,
            add_root_gitignore: !root_gitignore_present,
            write_agent_shim,
            register_mcp,
            install_agent_hooks: false,
            embedding_setup: EmbeddingSetupChoice::Disabled,
            embeddings_only: false,
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
    /// plan's `mode`/`target`/`embedding_setup` fields are placeholders;
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

    /// Build a state positioned directly at the embeddings picker, used by
    /// dashboard `T` when semantic triage is currently disabled.
    pub fn embeddings_only() -> Self {
        let mut s = Self::with_explain_support(Mode::Auto, vec![], ExplainWizardSupport::default());
        s.step = SetupStep::SelectEmbeddings;
        s.embeddings_only = true;
        s.add_root_gitignore = false;
        s.write_agent_shim = false;
        s.register_mcp = false;
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
            SetupStep::SelectActions => self.handle_select_actions_key(code, modifiers),
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
            flow: self.flow,
            mode: self.mode,
            target: self.target,
            add_root_gitignore: self.add_root_gitignore,
            write_agent_shim: self.write_agent_shim,
            register_mcp: self.register_mcp,
            install_agent_hooks: self.install_agent_hooks,
            embedding_setup: self.embedding_setup,
            explain: self.explain.clone(),
            reconcile_after: self.flow.initializes_runtime(),
        })
    }
}

fn default_agent_actions_for(
    current: &AgentIntegration,
    target: Option<AgentTargetKind>,
) -> (bool, bool) {
    let Some(target) = target else {
        return (false, false);
    };
    let (write_agent_shim, register_mcp) = match current {
        AgentIntegration::Complete { target: t } if *t == target => (false, false),
        AgentIntegration::Partial { target: t } if *t == target => (false, true),
        AgentIntegration::McpOnly { target: t } if *t == target => (true, false),
        _ => (true, true),
    };
    let register_mcp = register_mcp && target_tier(target) == AgentTargetTier::Automated;
    (write_agent_shim, register_mcp)
}
