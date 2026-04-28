//! Setup wizard type definitions: plan, outcome, steps, and state struct.

use super::explain::{
    CloudProvider, ExplainChoice, ExplainWizardSupport, LocalPreset, TextInputField,
};
use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::config::Mode;

/// Plan produced by a completed setup wizard. Executed by the bin-side
/// dispatcher after the TUI alternate-screen has been torn down.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupPlan {
    /// Config mode to write into `.synrepo/config.toml`.
    pub mode: Mode,
    /// Optional agent-integration target. `None` means the user chose "skip".
    pub target: Option<AgentTargetKind>,
    /// Explain decision. `None` means the user picked "Skip" (no
    /// `[explain]` block is written; keys in env stay untouched).
    pub explain: Option<ExplainChoice>,
    /// Whether to run a reconcile pass after init finishes. Always `true` for
    /// v1 so the first-run experience is complete; kept as a field so future
    /// "fast-path" flows can opt out without widening the plan shape.
    pub reconcile_after: bool,
}

/// Outcome returned by the setup wizard. Distinct from the dashboard's
/// [`crate::tui::TuiOutcome`] so the bin-side dispatcher can pattern-match on
/// the plan without reading an `Option` through `TuiOutcome`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupWizardOutcome {
    /// Stdout is not a TTY; the wizard was not entered.
    NonTty,
    /// Operator cancelled before the confirm step. No writes performed.
    Cancelled,
    /// Operator confirmed; caller must execute `plan`.
    Completed {
        /// Plan to execute.
        plan: SetupPlan,
    },
}

/// Steps the setup wizard walks through in order. The `Complete` step is the
/// terminal state that causes the render loop to exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupStep {
    /// One-screen welcome with a description, runtime estimate, and privacy
    /// reassurance. Enter advances; Esc / q / Ctrl-C cancels.
    Splash,
    /// Pick graph mode (auto or curated).
    SelectMode,
    /// Pick agent-integration target or "skip".
    SelectTarget,
    /// Static explainer: what explain produces, how it is triggered, what it
    /// costs, and the privacy posture. Always shown before `SelectExplain`.
    ExplainExplain,
    /// Pick explain provider (cloud, local, or skip).
    SelectExplain,
    /// Enter an API key for the selected cloud provider. Masked on screen;
    /// value is held in-memory only until the final apply step.
    EditCloudApiKey,
    /// Pick a local-LLM preset (Ollama, llama.cpp, LM Studio, vLLM, Custom).
    SelectLocalPreset,
    /// Edit the local-LLM endpoint URL. Pre-filled from the preset default.
    EditLocalEndpoint,
    /// "What you'll get / what you won't" review of the committed explain
    /// choice. Shown between committing a provider and the final `Confirm`
    /// step; skipped when explain is disabled.
    ReviewExplainPlan,
    /// Review the plan and press Enter to apply.
    Confirm,
    /// Render loop should exit; outcome is already captured.
    Complete,
}

/// V1 target roster offered by the wizard. Wider `AgentTool` variants
/// (Gemini, Goose, Kiro, etc.) stay behind `synrepo agent-setup <tool>` for
/// now; the wizard only offers the observationally-detectable five.
pub const WIZARD_TARGETS: &[AgentTargetKind] = &[
    AgentTargetKind::Claude,
    AgentTargetKind::Cursor,
    AgentTargetKind::Codex,
    AgentTargetKind::Copilot,
    AgentTargetKind::Windsurf,
];

/// State machine driving the setup wizard. Tests drive this struct directly
/// via [`SetupWizardState::handle_key`] and assert on `finalize()` /
/// `cancelled` / `step`.
#[derive(Clone, Debug)]
pub struct SetupWizardState {
    /// Current step.
    pub step: SetupStep,
    /// Cursor index in the mode list: 0 = Auto, 1 = Curated.
    pub mode_cursor: usize,
    /// Cursor index in the target list: 0..N for targets, N for "Skip".
    pub target_cursor: usize,
    /// Cursor index into [`crate::tui::wizard::setup::EXPLAIN_ROWS`].
    pub explain_cursor: usize,
    /// Cursor index into [`crate::tui::wizard::setup::LOCAL_PRESETS`].
    pub local_preset_cursor: usize,
    /// Committed mode (set on Enter at `SelectMode`).
    pub mode: Mode,
    /// Committed target (set on Enter at `SelectTarget`). `None` means skip.
    pub target: Option<AgentTargetKind>,
    /// Committed explain choice. `None` means the user picked "Skip" on
    /// `SelectExplain`. Set on Enter at `SelectExplain` for cloud/skip or
    /// at `EditLocalEndpoint` for local.
    pub explain: Option<ExplainChoice>,
    /// Text input buffer used by `EditLocalEndpoint`. Seeded with the preset
    /// default; mutable while the user edits.
    pub endpoint_input: TextInputField,
    /// Text input buffer used by `EditCloudApiKey`. Always masked on screen.
    pub api_key_input: TextInputField,
    /// Preset selected on `SelectLocalPreset`. Used by `EditLocalEndpoint` to
    /// build the final [`ExplainChoice::Local`] on Enter.
    pub local_preset: LocalPreset,
    /// Cloud provider currently being configured.
    pub pending_cloud_provider: Option<CloudProvider>,
    /// Observed reusable explain defaults from env/global config.
    pub explain_support: ExplainWizardSupport,
    /// Deterministic ordered list of agent targets detected from repo /
    /// `$HOME` hints. Used to pre-select the target cursor.
    pub detected_targets: Vec<AgentTargetKind>,
    /// True when the operator pressed Esc / Ctrl-C / q before Confirm.
    pub cancelled: bool,
}
