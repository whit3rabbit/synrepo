//! Setup wizard state types.

use crossterm::event::{KeyCode, KeyModifiers};

use super::synthesis::{
    LocalPreset, SynthesisChoice, SynthesisRow, TextInputField, LOCAL_PRESETS, SYNTHESIS_ROWS,
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
    /// Synthesis decision. `None` means the user picked "Skip" (no
    /// `[synthesis]` block is written; keys in env stay untouched).
    pub synthesis: Option<SynthesisChoice>,
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
    /// Pick synthesis provider (cloud, local, or skip).
    SelectSynthesis,
    /// Pick a local-LLM preset (Ollama, llama.cpp, LM Studio, vLLM, Custom).
    SelectLocalPreset,
    /// Edit the local-LLM endpoint URL. Pre-filled from the preset default.
    EditLocalEndpoint,
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
    /// Cursor index into [`SYNTHESIS_ROWS`].
    pub synthesis_cursor: usize,
    /// Cursor index into [`LOCAL_PRESETS`].
    pub local_preset_cursor: usize,
    /// Committed mode (set on Enter at `SelectMode`).
    pub mode: Mode,
    /// Committed target (set on Enter at `SelectTarget`). `None` means skip.
    pub target: Option<AgentTargetKind>,
    /// Committed synthesis choice. `None` means the user picked "Skip" on
    /// `SelectSynthesis`. Set on Enter at `SelectSynthesis` for cloud/skip or
    /// at `EditLocalEndpoint` for local.
    pub synthesis: Option<SynthesisChoice>,
    /// Text input buffer used by `EditLocalEndpoint`. Seeded with the preset
    /// default; mutable while the user edits.
    pub endpoint_input: TextInputField,
    /// Preset selected on `SelectLocalPreset`. Used by `EditLocalEndpoint` to
    /// build the final [`SynthesisChoice::Local`] on Enter.
    pub local_preset: LocalPreset,
    /// Deterministic ordered list of agent targets detected from repo /
    /// `$HOME` hints. Used to pre-select the target cursor.
    pub detected_targets: Vec<AgentTargetKind>,
    /// True when the operator pressed Esc / Ctrl-C / q before Confirm.
    pub cancelled: bool,
}

impl SetupWizardState {
    /// Build a fresh state. `default_mode` seeds the mode cursor; a caller
    /// that detected concept directories passes `Mode::Curated`.
    /// `detected_targets` seeds the target cursor to the first detected
    /// target, falling back to position 0 (the first target in
    /// [`WIZARD_TARGETS`]).
    pub fn new(default_mode: Mode, detected_targets: Vec<AgentTargetKind>) -> Self {
        let mode_cursor = match default_mode {
            Mode::Auto => 0,
            Mode::Curated => 1,
        };
        let target_cursor = detected_targets
            .first()
            .and_then(|t| WIZARD_TARGETS.iter().position(|wt| wt == t))
            .unwrap_or(0);
        Self {
            step: SetupStep::Splash,
            mode_cursor,
            target_cursor,
            synthesis_cursor: 0,
            local_preset_cursor: 0,
            mode: default_mode,
            target: None,
            synthesis: None,
            endpoint_input: TextInputField::with_value(LocalPreset::Ollama.default_endpoint()),
            local_preset: LocalPreset::Ollama,
            detected_targets,
            cancelled: false,
        }
    }

    /// Handle a key event; returns `true` if the loop should redraw. Pressing
    /// Esc / Ctrl-C / q at any step before `Confirm` cancels the wizard. At
    /// `Confirm`, Esc / b steps back rather than cancelling; Ctrl-C still
    /// cancels.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let is_quit = matches!(code, KeyCode::Esc | KeyCode::Char('q'))
            || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL));

        match self.step {
            SetupStep::Splash => {
                if is_quit {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                match code {
                    KeyCode::Enter => {
                        self.step = SetupStep::SelectMode;
                        true
                    }
                    _ => false,
                }
            }
            SetupStep::SelectMode => {
                if is_quit {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.mode_cursor = self.mode_cursor.saturating_sub(1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.mode_cursor < 1 {
                            self.mode_cursor += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        self.mode = if self.mode_cursor == 0 {
                            Mode::Auto
                        } else {
                            Mode::Curated
                        };
                        self.step = SetupStep::SelectTarget;
                        true
                    }
                    _ => false,
                }
            }
            SetupStep::SelectTarget => {
                if is_quit {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                let max = WIZARD_TARGETS.len(); // N = skip position
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.target_cursor = self.target_cursor.saturating_sub(1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.target_cursor < max {
                            self.target_cursor += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        self.target = WIZARD_TARGETS.get(self.target_cursor).copied();
                        self.step = SetupStep::SelectSynthesis;
                        true
                    }
                    _ => false,
                }
            }
            SetupStep::SelectSynthesis => {
                if is_quit {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                let max = SYNTHESIS_ROWS.len().saturating_sub(1);
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.synthesis_cursor = self.synthesis_cursor.saturating_sub(1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.synthesis_cursor < max {
                            self.synthesis_cursor += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        match SYNTHESIS_ROWS.get(self.synthesis_cursor).copied() {
                            Some(SynthesisRow::Skip) => {
                                self.synthesis = None;
                                self.step = SetupStep::Confirm;
                            }
                            Some(SynthesisRow::Cloud(provider)) => {
                                self.synthesis = Some(SynthesisChoice::Cloud(provider));
                                self.step = SetupStep::Confirm;
                            }
                            Some(SynthesisRow::Local) => {
                                self.step = SetupStep::SelectLocalPreset;
                            }
                            None => {}
                        }
                        true
                    }
                    _ => false,
                }
            }
            SetupStep::SelectLocalPreset => {
                if is_quit {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                let max = LOCAL_PRESETS.len().saturating_sub(1);
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.local_preset_cursor = self.local_preset_cursor.saturating_sub(1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.local_preset_cursor < max {
                            self.local_preset_cursor += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        let preset = LOCAL_PRESETS
                            .get(self.local_preset_cursor)
                            .copied()
                            .unwrap_or(LocalPreset::Ollama);
                        self.local_preset = preset;
                        // Re-seed the text field with the freshly chosen
                        // preset default so switching presets mid-flow is
                        // observable.
                        self.endpoint_input.reset(preset.default_endpoint());
                        self.step = SetupStep::EditLocalEndpoint;
                        true
                    }
                    _ => false,
                }
            }
            SetupStep::EditLocalEndpoint => {
                let is_abort =
                    code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL);
                if is_abort {
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    return true;
                }
                match code {
                    KeyCode::Esc => {
                        // Back to preset selection; do not cancel.
                        self.step = SetupStep::SelectLocalPreset;
                        true
                    }
                    KeyCode::Enter => {
                        let endpoint = self.endpoint_input.value().trim().to_string();
                        if endpoint.is_empty() {
                            // Silently refuse empty input; render layer will
                            // hint at this. Keep the step unchanged.
                            return false;
                        }
                        self.synthesis = Some(SynthesisChoice::Local {
                            preset: self.local_preset,
                            endpoint,
                        });
                        self.step = SetupStep::Confirm;
                        true
                    }
                    _ => self.endpoint_input.handle_key(code, modifiers),
                }
            }
            SetupStep::Confirm => match code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.step = SetupStep::Complete;
                    true
                }
                KeyCode::Esc | KeyCode::Char('b') => {
                    // Back to synthesis selection; do not cancel.
                    self.step = SetupStep::SelectSynthesis;
                    true
                }
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    // Explicit abort at confirm still cancels.
                    self.cancelled = true;
                    self.step = SetupStep::Complete;
                    true
                }
                _ => false,
            },
            SetupStep::Complete => false,
        }
    }

    /// If the state machine completed without cancelling, produce the plan.
    pub fn finalize(&self) -> Option<SetupPlan> {
        if self.cancelled || self.step != SetupStep::Complete {
            return None;
        }
        Some(SetupPlan {
            mode: self.mode,
            target: self.target,
            synthesis: self.synthesis.clone(),
            reconcile_after: true,
        })
    }
}
