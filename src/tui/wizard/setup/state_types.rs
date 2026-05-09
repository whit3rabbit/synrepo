//! Setup wizard type definitions: plan, outcome, steps, and state struct.

use super::explain::{
    CloudProvider, ExplainChoice, ExplainWizardSupport, LocalPreset, TextInputField,
};
use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::config::Mode;

/// Embedding setup decision made by the wizard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingSetupChoice {
    /// Leave optional semantic routing/search disabled.
    Disabled,
    /// Enable the built-in ONNX backend.
    Onnx,
    /// Enable local Ollama `/api/embed`.
    Ollama,
}

impl EmbeddingSetupChoice {
    /// Whether this choice enables semantic triage.
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

/// Setup wizard shape. Full setup initializes runtime and optional providers;
/// follow-up setup only offers repo-local integration and gitignore actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupFlow {
    /// First-run or explicit full setup.
    Full,
    /// Ready repo follow-up for missing integration, hooks, or root gitignore.
    FollowUp,
}

impl SetupFlow {
    /// Whether this flow should initialize or refresh `.synrepo/`.
    pub fn initializes_runtime(self) -> bool {
        matches!(self, Self::Full)
    }
}

/// Action rows on the setup wizard's repo-local action step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupActionRow {
    /// Add `.synrepo/` to the repository root `.gitignore`.
    AddRootGitignore,
    /// Write the agent skill or instructions file.
    WriteAgentShim,
    /// Register the repo-local MCP server for automated targets.
    RegisterMcp,
    /// Install local client nudge hooks.
    InstallAgentHooks,
}

/// Fixed action row order used by state and rendering.
pub const SETUP_ACTION_ROWS: &[SetupActionRow] = &[
    SetupActionRow::AddRootGitignore,
    SetupActionRow::WriteAgentShim,
    SetupActionRow::RegisterMcp,
    SetupActionRow::InstallAgentHooks,
];

/// Plan produced by a completed setup wizard. Executed by the bin-side
/// dispatcher after the TUI alternate-screen has been torn down.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupPlan {
    /// Wizard shape that produced this plan.
    pub flow: SetupFlow,
    /// Config mode to write into `.synrepo/config.toml`.
    pub mode: Mode,
    /// Optional agent-integration target. `None` means the user chose "skip".
    pub target: Option<AgentTargetKind>,
    /// Add `.synrepo/` to the repository root `.gitignore`.
    pub add_root_gitignore: bool,
    /// Write or preserve the selected agent skill/instructions.
    pub write_agent_shim: bool,
    /// Register a repo-local MCP server for the selected agent.
    pub register_mcp: bool,
    /// Install local nudge hooks for supported agents.
    pub install_agent_hooks: bool,
    /// Optional embedding backend to configure for semantic routing and
    /// hybrid search in this repository.
    pub embedding_setup: EmbeddingSetupChoice,
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
    /// Pick repo-local integration actions and root gitignore handling.
    SelectActions,
    /// Pick optional embeddings setup or leave semantic triage disabled.
    SelectEmbeddings,
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

/// Target roster offered by the wizard. This mirrors the local agent-config
/// registry coverage exposed by the binary-side `AgentTool` catalog.
pub const WIZARD_TARGETS: &[AgentTargetKind] = &[
    AgentTargetKind::Claude,
    AgentTargetKind::Cursor,
    AgentTargetKind::Codex,
    AgentTargetKind::Copilot,
    AgentTargetKind::Windsurf,
    AgentTargetKind::Amp,
    AgentTargetKind::Antigravity,
    AgentTargetKind::Cline,
    AgentTargetKind::CodeBuddy,
    AgentTargetKind::Crush,
    AgentTargetKind::Forge,
    AgentTargetKind::Gemini,
    AgentTargetKind::Hermes,
    AgentTargetKind::Iflow,
    AgentTargetKind::Junie,
    AgentTargetKind::Kilocode,
    AgentTargetKind::Opencode,
    AgentTargetKind::Openclaw,
    AgentTargetKind::Pi,
    AgentTargetKind::Qodercli,
    AgentTargetKind::Qwen,
    AgentTargetKind::Roo,
    AgentTargetKind::Tabnine,
    AgentTargetKind::Trae,
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
    /// Cursor index in the repo-local actions list.
    pub action_cursor: usize,
    /// Cursor index in the embeddings list: 0 = Skip, 1 = ONNX, 2 = Ollama.
    pub embeddings_cursor: usize,
    /// Cursor index into [`crate::tui::wizard::setup::EXPLAIN_ROWS`].
    pub explain_cursor: usize,
    /// Cursor index into [`crate::tui::wizard::setup::LOCAL_PRESETS`].
    pub local_preset_cursor: usize,
    /// Committed mode (set on Enter at `SelectMode`).
    pub mode: Mode,
    /// Committed target (set on Enter at `SelectTarget`). `None` means skip.
    pub target: Option<AgentTargetKind>,
    /// Setup wizard shape.
    pub flow: SetupFlow,
    /// Probe-derived current integration state for default action selection.
    pub current_integration: AgentIntegration,
    /// Whether the root `.gitignore` already contains `.synrepo/`.
    pub root_gitignore_present: bool,
    /// Committed gitignore action.
    pub add_root_gitignore: bool,
    /// Committed action: write/preserve the selected agent shim.
    pub write_agent_shim: bool,
    /// Committed action: register repo-local MCP for selected target.
    pub register_mcp: bool,
    /// Committed action: install local nudge hooks.
    pub install_agent_hooks: bool,
    /// Committed embeddings choice.
    pub embedding_setup: EmbeddingSetupChoice,
    /// True when running only the embeddings setup sub-flow.
    pub embeddings_only: bool,
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
