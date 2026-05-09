//! Per-step key handlers for the setup wizard state machine.

use crossterm::event::{KeyCode, KeyModifiers};

use super::super::explain::{
    CloudCredentialSource, CloudProvider, ExplainChoice, ExplainRow, LocalPreset, EXPLAIN_ROWS,
    LOCAL_PRESETS,
};
use super::super::state_types::{
    EmbeddingSetupChoice, SetupStep, SetupWizardState, WIZARD_TARGETS,
};
use crate::config::Mode;

impl SetupWizardState {
    pub(super) fn handle_splash_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
        }
        match code {
            KeyCode::Enter => {
                self.step = SetupStep::SelectMode;
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_select_mode_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
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

    pub(super) fn handle_select_target_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
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
                self.reseed_target_actions();
                self.step = SetupStep::SelectActions;
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_select_embeddings_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.embeddings_cursor = self.embeddings_cursor.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.embeddings_cursor < 2 {
                    self.embeddings_cursor += 1;
                }
                true
            }
            KeyCode::Char('b') => {
                if self.embeddings_only {
                    return false;
                }
                self.step = SetupStep::SelectActions;
                true
            }
            KeyCode::Enter => {
                self.embedding_setup = match self.embeddings_cursor {
                    1 => EmbeddingSetupChoice::Onnx,
                    2 => EmbeddingSetupChoice::Ollama,
                    _ => EmbeddingSetupChoice::Disabled,
                };
                self.step = if self.embeddings_only {
                    SetupStep::Complete
                } else {
                    SetupStep::ExplainExplain
                };
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_explain_explain_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
        }
        match code {
            KeyCode::Enter => {
                self.step = SetupStep::SelectExplain;
                true
            }
            KeyCode::Char('b') => {
                self.step = SetupStep::SelectEmbeddings;
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_select_explain_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
        }
        let max = EXPLAIN_ROWS.len().saturating_sub(1);
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.explain_cursor = self.explain_cursor.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.explain_cursor < max {
                    self.explain_cursor += 1;
                }
                true
            }
            KeyCode::Enter => {
                match EXPLAIN_ROWS.get(self.explain_cursor).copied() {
                    Some(ExplainRow::Skip) => {
                        self.explain = None;
                        self.pending_cloud_provider = None;
                        // Nothing to review when explain is off.
                        self.step = SetupStep::Confirm;
                    }
                    Some(ExplainRow::Cloud(provider)) => {
                        self.pending_cloud_provider = Some(provider);
                        self.api_key_input.reset("");
                        if let Some(credential_source) =
                            self.explain_support.credential_source_for(provider)
                        {
                            self.explain = Some(ExplainChoice::Cloud {
                                provider,
                                credential_source,
                                api_key: None,
                            });
                            self.step = SetupStep::ReviewExplainPlan;
                        } else {
                            self.explain = None;
                            self.step = SetupStep::EditCloudApiKey;
                        }
                    }
                    Some(ExplainRow::Local) => {
                        self.pending_cloud_provider = None;
                        self.step = SetupStep::SelectLocalPreset;
                    }
                    None => {}
                }
                true
            }
            _ => false,
        }
    }

    // Text-input step: only Ctrl-C cancels; Esc and q are either a step-back
    // or valid characters for the text field.
    pub(super) fn handle_edit_cloud_api_key_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return self.cancel_to_complete();
        }
        match code {
            KeyCode::Esc => {
                self.pending_cloud_provider = None;
                self.api_key_input.reset("");
                self.step = SetupStep::SelectExplain;
                true
            }
            KeyCode::Enter => {
                let api_key = self.api_key_input.value().trim().to_string();
                if api_key.is_empty() {
                    return false;
                }
                let provider = self
                    .pending_cloud_provider
                    .unwrap_or(CloudProvider::Anthropic);
                self.explain = Some(ExplainChoice::Cloud {
                    provider,
                    credential_source: CloudCredentialSource::EnteredGlobal,
                    api_key: Some(api_key),
                });
                self.step = SetupStep::ReviewExplainPlan;
                true
            }
            _ => self.api_key_input.handle_key(code, modifiers),
        }
    }

    pub(super) fn handle_select_local_preset_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
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
                // Re-seed the text field with the freshly chosen preset
                // default so switching presets mid-flow is observable.
                self.endpoint_input.reset(preset.default_endpoint());
                self.step = SetupStep::EditLocalEndpoint;
                true
            }
            _ => false,
        }
    }

    // Text-input step: only Ctrl-C cancels; Esc steps back to preset list.
    pub(super) fn handle_edit_local_endpoint_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return self.cancel_to_complete();
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
                    // Silently refuse empty input; render layer will hint at
                    // this. Keep the step unchanged.
                    return false;
                }
                self.explain = Some(ExplainChoice::Local {
                    preset: self.local_preset,
                    endpoint,
                });
                self.step = SetupStep::ReviewExplainPlan;
                true
            }
            _ => self.endpoint_input.handle_key(code, modifiers),
        }
    }

    pub(super) fn handle_review_explain_plan_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if Self::is_quit_key(code, modifiers) {
            return self.cancel_to_complete();
        }
        match code {
            KeyCode::Enter => {
                self.step = SetupStep::Confirm;
                true
            }
            KeyCode::Char('b') => {
                // Jump back to the provider-selection step. Clear the
                // committed choice so a back-forward round trip never
                // silently reuses a stale selection.
                self.explain = None;
                self.pending_cloud_provider = None;
                self.api_key_input.reset("");
                self.step = SetupStep::SelectExplain;
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_confirm_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Enter | KeyCode::Char('y') => {
                self.step = SetupStep::Complete;
                true
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                // Back one step: to actions in follow-up mode, to the review
                // screen when a provider was chosen, otherwise to explain.
                self.step = if self.flow == super::SetupFlow::FollowUp {
                    SetupStep::SelectActions
                } else if self.explain.is_some() {
                    SetupStep::ReviewExplainPlan
                } else {
                    SetupStep::SelectExplain
                };
                true
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                // Explicit abort at confirm still cancels.
                self.cancel_to_complete()
            }
            _ => false,
        }
    }
}
