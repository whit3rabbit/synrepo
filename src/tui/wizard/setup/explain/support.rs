//! Explain support detection and wizard choice types.

use super::providers::{CloudProvider, LocalPreset};
use crate::config::{Config, ExplainConfig};

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

/// Best-effort explain defaults observed from the process environment and
/// user-scoped config before the wizard starts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplainWizardSupport {
    global_explain: ExplainConfig,
}

impl ExplainWizardSupport {
    /// Construct support state from an explicit global explain config. Used
    /// by unit tests to avoid touching the real home directory.
    pub fn with_global_explain(global_explain: ExplainConfig) -> Self {
        Self { global_explain }
    }

    /// Read the user-scoped config at `~/.synrepo/config.toml`. Parse errors
    /// degrade to defaults so setup can still run; apply-time writes still
    /// fail loudly rather than clobbering invalid TOML.
    pub fn detect() -> Self {
        let global_path = Config::global_config_path();
        let global_explain = std::fs::read_to_string(&global_path)
            .ok()
            .and_then(|text| {
                toml::from_str::<crate::config::Config>(&text)
                    .map(|config| config.explain)
                    .map_err(|error| {
                        tracing::warn!(
                            "setup: ignoring unreadable global explain config at {} ({error})",
                            global_path.display()
                        );
                        error
                    })
                    .ok()
            })
            .unwrap_or_default();
        Self { global_explain }
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
        } else if provider.saved_key(&self.global_explain).is_some() {
            Some(CloudCredentialSource::SavedGlobal)
        } else {
            None
        }
    }

    /// Seed the local-provider preset from the user-scoped config when
    /// present so repeated setup runs start from the last saved endpoint.
    pub fn local_preset(&self) -> LocalPreset {
        self.global_explain
            .local_preset
            .as_deref()
            .and_then(LocalPreset::from_config_value)
            .unwrap_or(LocalPreset::Ollama)
    }

    /// Seed the local-provider endpoint from the user-scoped config when
    /// present.
    pub fn local_endpoint(&self) -> Option<&str> {
        self.global_explain.local_endpoint.as_deref()
    }
}

/// The user's explain decision captured on the plan. `None` on the plan
/// means the user selected "Skip" (no `[explain]` block written).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplainChoice {
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

/// Row in the explain selection list. Order is stable; tests index into
/// this by position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplainRow {
    /// Skip explain; no `[explain]` block is written.
    Skip,
    /// Pick a cloud provider.
    Cloud(CloudProvider),
    /// Pick the local sub-flow.
    Local,
}

/// Rows rendered on the explain selection step, in order.
pub const EXPLAIN_ROWS: &[ExplainRow] = &[
    ExplainRow::Skip,
    ExplainRow::Cloud(CloudProvider::Anthropic),
    ExplainRow::Cloud(CloudProvider::OpenAi),
    ExplainRow::Cloud(CloudProvider::Gemini),
    ExplainRow::Cloud(CloudProvider::OpenRouter),
    ExplainRow::Cloud(CloudProvider::Zai),
    ExplainRow::Cloud(CloudProvider::Minimax),
    ExplainRow::Local,
];

impl ExplainRow {
    /// Label rendered in the list.
    pub fn label(&self) -> &'static str {
        match self {
            ExplainRow::Skip => "Skip — leave explain disabled (recommended default)",
            ExplainRow::Cloud(p) => p.label(),
            ExplainRow::Local => "Local LLM (Ollama, llama.cpp, LM Studio, vLLM)",
        }
    }
}
