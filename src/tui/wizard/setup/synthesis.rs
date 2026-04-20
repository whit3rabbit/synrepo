//! Synthesis-step types for the setup wizard.
//!
//! Kept out of `state.rs` so the state machine file stays under the 400-line
//! limit and so the synthesis choice + local-endpoint presets have a single
//! place to live. The plan shape (`SynthesisChoice`) is also what the bin-side
//! dispatcher pattern-matches on when patching repo-local `.synrepo/config.toml`
//! and user-scoped `~/.synrepo/config.toml`.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::config::{Config, SynthesisConfig};

/// Cloud synthesis providers offered by the wizard. Maps 1:1 to
/// `config.synthesis.provider` string values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudProvider {
    /// Anthropic (Claude).
    Anthropic,
    /// OpenAI (ChatGPT).
    OpenAi,
    /// Google Gemini.
    Gemini,
    /// OpenRouter.
    OpenRouter,
    /// Z.ai (Zhipu GLM).
    Zai,
    /// MiniMax (international endpoint).
    Minimax,
}

impl CloudProvider {
    /// Config string written under `[synthesis] provider = "<this>"`.
    pub fn config_value(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => "anthropic",
            CloudProvider::OpenAi => "openai",
            CloudProvider::Gemini => "gemini",
            CloudProvider::OpenRouter => "openrouter",
            CloudProvider::Zai => "zai",
            CloudProvider::Minimax => "minimax",
        }
    }

    /// Human-readable label shown in the provider picker.
    pub fn label(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => "Anthropic (Claude)",
            CloudProvider::OpenAi => "OpenAI",
            CloudProvider::Gemini => "Gemini",
            CloudProvider::OpenRouter => "OpenRouter",
            CloudProvider::Zai => "Z.ai (Zhipu GLM)",
            CloudProvider::Minimax => "MiniMax",
        }
    }

    /// Matching environment variable name for this provider's API key.
    pub fn env_var(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => "ANTHROPIC_API_KEY",
            CloudProvider::OpenAi => "OPENAI_API_KEY",
            CloudProvider::Gemini => "GEMINI_API_KEY",
            CloudProvider::OpenRouter => "OPENROUTER_API_KEY",
            CloudProvider::Zai => "ZAI_API_KEY",
            CloudProvider::Minimax => "MINIMAX_API_KEY",
        }
    }

    /// Key written into `[synthesis]` when setup persists a reusable key in
    /// `~/.synrepo/config.toml`.
    pub fn api_key_field(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => "anthropic_api_key",
            CloudProvider::OpenAi => "openai_api_key",
            CloudProvider::Gemini => "gemini_api_key",
            CloudProvider::OpenRouter => "openrouter_api_key",
            CloudProvider::Zai => "zai_api_key",
            CloudProvider::Minimax => "minimax_api_key",
        }
    }

    fn saved_key<'a>(&self, config: &'a SynthesisConfig) -> Option<&'a str> {
        match self {
            CloudProvider::Anthropic => config.anthropic_api_key.as_deref(),
            CloudProvider::OpenAi => config.openai_api_key.as_deref(),
            CloudProvider::Gemini => config.gemini_api_key.as_deref(),
            CloudProvider::OpenRouter => config.openrouter_api_key.as_deref(),
            CloudProvider::Zai => config.zai_api_key.as_deref(),
            CloudProvider::Minimax => config.minimax_api_key.as_deref(),
        }
    }

    /// One-sentence description of what the provider is best at.
    /// Rendered on the `ExplainSynthesis` step and on the review screen.
    pub fn description(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => {
                "Frontier-tier Claude models. Strong code explanations, high-quality cross-link candidates."
            }
            CloudProvider::OpenAi => {
                "OpenAI hosted models. Widely available keys; quality varies by the model you select."
            }
            CloudProvider::Gemini => {
                "Google Gemini hosted models. Longer context windows; good fit for large files."
            }
            CloudProvider::OpenRouter => {
                "Unified billing across dozens of frontier and open-source models via one key."
            }
            CloudProvider::Zai => {
                "Zhipu's GLM models via Z.ai's OpenAI-compatible endpoint. GLM-4.6 is the current agentic-coding flagship."
            }
            CloudProvider::Minimax => {
                "MiniMax's OpenAI-compatible endpoint. MiniMax-M2 is positioned as an inexpensive agentic-coding option."
            }
        }
    }

    /// Order-of-magnitude cost expectation per full refresh on a 500-symbol
    /// repo. Deliberately rough: rates shift and we refuse to quote precise
    /// numbers without reading the provider's live rate card. Surfaced in the
    /// wizard's explainer and review steps.
    pub fn cost_hint(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => {
                "Typically a few cents per full refresh on a frontier model; your API key is billed directly."
            }
            CloudProvider::OpenAi => {
                "Typically a few cents per full refresh; cheap with `gpt-4o-mini`, higher with larger models."
            }
            CloudProvider::Gemini => {
                "Flash-tier models are cheap; Pro-tier costs more and is billed via Google Cloud."
            }
            CloudProvider::OpenRouter => {
                "Cost depends entirely on which underlying model you pick; OpenRouter's docs list live rates."
            }
            CloudProvider::Zai => {
                "GLM-4.6 lists at ~$0.60 input / $2.20 output per 1M tokens; GLM-4.5-Air is ~3x cheaper."
            }
            CloudProvider::Minimax => {
                "MiniMax-M2 lists at ~$0.30 input / $1.20 output per 1M tokens at launch; cheaper than most frontier models."
            }
        }
    }
}

/// Local-LLM server presets. Each maps to a default endpoint URL and a
/// `local_preset` string written to config for informational use. The
/// endpoint is authoritative for dispatch; the preset label is display-only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalPreset {
    /// Ollama (native `/api/chat`).
    Ollama,
    /// llama.cpp (OpenAI-compatible `/v1/chat/completions`).
    LlamaCpp,
    /// LM Studio (OpenAI-compatible `/v1/chat/completions`).
    LmStudio,
    /// vLLM (OpenAI-compatible `/v1/chat/completions`).
    Vllm,
    /// Custom endpoint: user provides the URL.
    Custom,
}

impl LocalPreset {
    /// Stable preset id for `config.synthesis.local_preset`.
    pub fn config_value(&self) -> &'static str {
        match self {
            LocalPreset::Ollama => "ollama",
            LocalPreset::LlamaCpp => "llamacpp",
            LocalPreset::LmStudio => "lmstudio",
            LocalPreset::Vllm => "vllm",
            LocalPreset::Custom => "custom",
        }
    }

    /// Default endpoint URL the text-input step is pre-filled with.
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            LocalPreset::Ollama => "http://localhost:11434/api/chat",
            LocalPreset::LlamaCpp => "http://localhost:8080/v1/chat/completions",
            LocalPreset::LmStudio => "http://localhost:1234/v1/chat/completions",
            LocalPreset::Vllm => "http://localhost:8000/v1/chat/completions",
            LocalPreset::Custom => "http://localhost:11434/api/chat",
        }
    }

    /// Human-readable label shown in the preset list.
    pub fn label(&self) -> &'static str {
        match self {
            LocalPreset::Ollama => "Ollama (native /api/chat)",
            LocalPreset::LlamaCpp => "llama.cpp server (OpenAI-compatible)",
            LocalPreset::LmStudio => "LM Studio (OpenAI-compatible)",
            LocalPreset::Vllm => "vLLM (OpenAI-compatible)",
            LocalPreset::Custom => "Custom endpoint",
        }
    }

    /// One-sentence description of what the preset is for.
    pub fn description(&self) -> &'static str {
        match self {
            LocalPreset::Ollama => {
                "Easiest setup: `ollama pull llama3` then start `ollama serve`. Reports exact token counts."
            }
            LocalPreset::LlamaCpp => {
                "llama.cpp's OpenAI-compatible server. Fast on consumer GPUs; token counts depend on the build."
            }
            LocalPreset::LmStudio => {
                "Desktop GUI with a local OpenAI-compatible server. Good for experimenting with many models."
            }
            LocalPreset::Vllm => {
                "High-throughput inference server for self-hosted deployments."
            }
            LocalPreset::Custom => "Point at any OpenAI-compatible or Ollama-native endpoint you run yourself.",
        }
    }

    /// Cost / privacy expectation for this preset. Local servers never bill,
    /// but output quality depends on the model the user pulled; make that
    /// explicit in the wizard.
    pub fn cost_hint(&self) -> &'static str {
        "No API cost: requests stay on your machine. Output quality depends on the model you pulled."
    }

    /// Parse the persisted `local_preset` string.
    pub fn from_config_value(raw: &str) -> Option<Self> {
        match raw {
            "ollama" => Some(LocalPreset::Ollama),
            "llamacpp" => Some(LocalPreset::LlamaCpp),
            "lmstudio" => Some(LocalPreset::LmStudio),
            "vllm" => Some(LocalPreset::Vllm),
            "custom" => Some(LocalPreset::Custom),
            _ => None,
        }
    }
}

/// All preset variants in display order.
pub const LOCAL_PRESETS: &[LocalPreset] = &[
    LocalPreset::Ollama,
    LocalPreset::LlamaCpp,
    LocalPreset::LmStudio,
    LocalPreset::Vllm,
    LocalPreset::Custom,
];

/// Where setup resolved a cloud provider's key from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudCredentialSource {
    /// API key is present in the current shell environment.
    Env,
    /// API key already exists in `~/.synrepo/config.toml`.
    SavedGlobal,
    /// User typed a new key during setup; apply will persist it globally.
    EnteredGlobal,
}

/// Best-effort synthesis defaults observed from the process environment and
/// user-scoped config before the wizard starts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SynthesisWizardSupport {
    global_synthesis: SynthesisConfig,
}

impl SynthesisWizardSupport {
    /// Construct support state from an explicit global synthesis config. Used
    /// by unit tests to avoid touching the real home directory.
    pub fn with_global_synthesis(global_synthesis: SynthesisConfig) -> Self {
        Self { global_synthesis }
    }

    /// Read the user-scoped config at `~/.synrepo/config.toml`. Parse errors
    /// degrade to defaults so setup can still run; apply-time writes still
    /// fail loudly rather than clobbering invalid TOML.
    pub fn detect() -> Self {
        let global_path = Config::global_config_path();
        let global_synthesis = std::fs::read_to_string(&global_path)
            .ok()
            .and_then(|text| {
                toml::from_str::<crate::config::Config>(&text)
                    .map(|config| config.synthesis)
                    .map_err(|error| {
                        tracing::warn!(
                            "setup: ignoring unreadable global synthesis config at {} ({error})",
                            global_path.display()
                        );
                        error
                    })
                    .ok()
            })
            .unwrap_or_default();
        Self { global_synthesis }
    }

    /// Resolve whether setup can reuse an existing cloud credential for the
    /// selected provider.
    pub fn credential_source_for(&self, provider: CloudProvider) -> Option<CloudCredentialSource> {
        if std::env::var(provider.env_var())
            .ok()
            .filter(|value| !value.is_empty())
            .is_some()
        {
            Some(CloudCredentialSource::Env)
        } else if provider.saved_key(&self.global_synthesis).is_some() {
            Some(CloudCredentialSource::SavedGlobal)
        } else {
            None
        }
    }

    /// Seed the local-provider preset from the user-scoped config when
    /// present so repeated setup runs start from the last saved endpoint.
    pub fn local_preset(&self) -> LocalPreset {
        self.global_synthesis
            .local_preset
            .as_deref()
            .and_then(LocalPreset::from_config_value)
            .unwrap_or(LocalPreset::Ollama)
    }

    /// Seed the local-provider endpoint from the user-scoped config when
    /// present.
    pub fn local_endpoint(&self) -> Option<&str> {
        self.global_synthesis.local_endpoint.as_deref()
    }
}

/// The user's synthesis decision captured on the plan. `None` on the plan
/// means the user selected "Skip" (no `[synthesis]` block written).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SynthesisChoice {
    /// A cloud provider, plus where setup resolved the credential.
    Cloud {
        /// Provider to enable.
        provider: CloudProvider,
        /// Whether setup is reusing env, reusing saved global config, or
        /// persisting a newly-entered key on apply.
        credential_source: CloudCredentialSource,
        /// Newly-entered API key. Only populated for `EnteredGlobal`.
        api_key: Option<String>,
    },
    /// Local provider with the selected preset and the (possibly edited)
    /// endpoint URL.
    Local {
        /// Preset the user started from.
        preset: LocalPreset,
        /// Final endpoint URL (may differ from the preset default if the
        /// user edited it).
        endpoint: String,
    },
}

/// Row in the synthesis selection list. Order is stable; tests index into
/// this by position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SynthesisRow {
    /// Skip synthesis; no `[synthesis]` block is written.
    Skip,
    /// Pick a cloud provider.
    Cloud(CloudProvider),
    /// Pick the local sub-flow.
    Local,
}

/// Rows rendered on the synthesis selection step, in order.
pub const SYNTHESIS_ROWS: &[SynthesisRow] = &[
    SynthesisRow::Skip,
    SynthesisRow::Cloud(CloudProvider::Anthropic),
    SynthesisRow::Cloud(CloudProvider::OpenAi),
    SynthesisRow::Cloud(CloudProvider::Gemini),
    SynthesisRow::Cloud(CloudProvider::OpenRouter),
    SynthesisRow::Cloud(CloudProvider::Zai),
    SynthesisRow::Cloud(CloudProvider::Minimax),
    SynthesisRow::Local,
];

impl SynthesisRow {
    /// Label rendered in the list.
    pub fn label(&self) -> &'static str {
        match self {
            SynthesisRow::Skip => "Skip — leave synthesis disabled (recommended default)",
            SynthesisRow::Cloud(p) => p.label(),
            SynthesisRow::Local => "Local LLM (Ollama, llama.cpp, LM Studio, vLLM)",
        }
    }
}

/// Single-line text input field used by the endpoint-edit step.
///
/// Deliberately narrow — one buffer, one cursor, Char / Backspace / Left /
/// Right / Home / End / Ctrl-U (clear). No multi-line, no selection, no
/// validation beyond "non-empty" at commit time. Tests drive it via
/// [`TextInputField::handle_key`] the same way they drive the rest of the
/// wizard state machine.
#[derive(Clone, Debug)]
pub struct TextInputField {
    buffer: String,
    cursor: usize,
}

impl TextInputField {
    /// Construct with a pre-filled value; cursor lands at end of text.
    pub fn with_value(initial: &str) -> Self {
        Self {
            buffer: initial.to_string(),
            cursor: initial.chars().count(),
        }
    }

    /// Replace the entire buffer and move the cursor to the end. Used when
    /// the user switches preset after already typing: the text input is
    /// re-seeded with the new preset's default endpoint.
    pub fn reset(&mut self, value: &str) {
        self.buffer = value.to_string();
        self.cursor = self.buffer.chars().count();
    }

    /// Borrow the current buffer contents.
    pub fn value(&self) -> &str {
        &self.buffer
    }

    /// Cursor position (in chars, not bytes).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Handle a key event. Returns `true` when the input was modified (the
    /// render loop should redraw). `Enter` and `Esc` are NOT handled here —
    /// the parent state machine observes them to drive the step transition.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.clear();
                self.cursor = 0;
                true
            }
            KeyCode::Char(c) => {
                // Unicode-safe insert at cursor position.
                let byte_index = self
                    .buffer
                    .char_indices()
                    .nth(self.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.buffer.len());
                self.buffer.insert(byte_index, c);
                self.cursor += 1;
                true
            }
            KeyCode::Backspace => {
                if self.cursor == 0 {
                    return false;
                }
                let prev_byte = self
                    .buffer
                    .char_indices()
                    .nth(self.cursor - 1)
                    .map(|(i, _)| i)
                    .expect("cursor > 0 implies a char exists");
                let this_byte = self
                    .buffer
                    .char_indices()
                    .nth(self.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.buffer.len());
                self.buffer.drain(prev_byte..this_byte);
                self.cursor -= 1;
                true
            }
            KeyCode::Left if self.cursor > 0 => {
                self.cursor -= 1;
                true
            }
            KeyCode::Left => false,
            KeyCode::Right if self.cursor < self.buffer.chars().count() => {
                self.cursor += 1;
                true
            }
            KeyCode::Right => false,
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                self.cursor = self.buffer.chars().count();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(f: &mut TextInputField, code: KeyCode) {
        f.handle_key(code, KeyModifiers::empty());
    }

    #[test]
    fn with_value_places_cursor_at_end() {
        let f = TextInputField::with_value("abc");
        assert_eq!(f.value(), "abc");
        assert_eq!(f.cursor(), 3);
    }

    #[test]
    fn typed_chars_insert_at_cursor() {
        let mut f = TextInputField::with_value("hello");
        press(&mut f, KeyCode::Home);
        press(&mut f, KeyCode::Char('!'));
        assert_eq!(f.value(), "!hello");
        assert_eq!(f.cursor(), 1);
    }

    #[test]
    fn backspace_removes_previous_char() {
        let mut f = TextInputField::with_value("abc");
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "ab");
        assert_eq!(f.cursor(), 2);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut f = TextInputField::with_value("x");
        press(&mut f, KeyCode::Home);
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "x");
        assert_eq!(f.cursor(), 0);
    }

    #[test]
    fn left_right_do_not_wrap() {
        let mut f = TextInputField::with_value("ab");
        press(&mut f, KeyCode::Right);
        assert_eq!(f.cursor(), 2);
        press(&mut f, KeyCode::Home);
        press(&mut f, KeyCode::Left);
        assert_eq!(f.cursor(), 0);
    }

    #[test]
    fn ctrl_u_clears_buffer() {
        let mut f = TextInputField::with_value("http://localhost");
        f.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(f.value(), "");
        assert_eq!(f.cursor(), 0);
    }

    #[test]
    fn reset_replaces_buffer() {
        let mut f = TextInputField::with_value("old");
        f.reset("brand-new-value");
        assert_eq!(f.value(), "brand-new-value");
        assert_eq!(f.cursor(), "brand-new-value".chars().count());
    }

    #[test]
    fn unicode_insert_and_backspace() {
        let mut f = TextInputField::with_value("");
        press(&mut f, KeyCode::Char('é'));
        press(&mut f, KeyCode::Char('x'));
        assert_eq!(f.value(), "éx");
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "é");
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "");
    }
}
