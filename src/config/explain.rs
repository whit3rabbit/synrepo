//! Explain pipeline configuration.
//!
//! Off by default. Opting in via `enabled = true` is required even when a
//! provider API key is present in the environment. Without this safeguard,
//! synrepo would silently consume any `ANTHROPIC_API_KEY` /
//! `OPENAI_API_KEY` / `GEMINI_API_KEY` the user happens to have set for
//! unrelated tools.

use serde::{Deserialize, Serialize};

/// Default number of concurrent commentary provider calls during refresh.
pub const DEFAULT_COMMENTARY_CONCURRENCY: usize = 4;

fn default_commentary_concurrency() -> usize {
    DEFAULT_COMMENTARY_CONCURRENCY
}

/// Explain provider configuration block persisted as `[explain]` in
/// `.synrepo/config.toml`. All fields are optional with serde defaults; missing
/// or absent block means "disabled, no preferences set". The legacy
/// `[synthesis]` block is rejected by `Config::load`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainConfig {
    /// When false, explain is disabled regardless of env keys. Set
    /// `SYNREPO_LLM_ENABLED=1` as a one-shot env override without persisting.
    #[serde(default)]
    pub enabled: bool,

    /// Provider selector: `anthropic | openai | gemini | openrouter | local | none`.
    /// `SYNREPO_LLM_PROVIDER` env var overrides this when set.
    #[serde(default)]
    pub provider: Option<String>,

    /// Anthropic API key. Shared user-scoped default lives in
    /// `~/.synrepo/config.toml`; env still wins.
    #[serde(default)]
    pub anthropic_api_key: Option<String>,

    /// OpenAI API key. Shared user-scoped default lives in
    /// `~/.synrepo/config.toml`; env still wins.
    #[serde(default)]
    pub openai_api_key: Option<String>,

    /// Gemini API key. Shared user-scoped default lives in
    /// `~/.synrepo/config.toml`; env still wins.
    #[serde(default)]
    pub gemini_api_key: Option<String>,

    /// OpenRouter API key. Shared user-scoped default lives in
    /// `~/.synrepo/config.toml`; env still wins.
    #[serde(default)]
    pub openrouter_api_key: Option<String>,

    /// Z.ai API key. Shared user-scoped default lives in
    /// `~/.synrepo/config.toml`; env still wins.
    #[serde(default)]
    pub zai_api_key: Option<String>,

    /// MiniMax API key. Shared user-scoped default lives in
    /// `~/.synrepo/config.toml`; env still wins.
    #[serde(default)]
    pub minimax_api_key: Option<String>,

    /// Model override. `SYNREPO_LLM_MODEL` env var overrides this.
    #[serde(default)]
    pub model: Option<String>,

    /// Endpoint URL for the `local` provider. Request shape is inferred
    /// from the path suffix: `/v1/chat/completions` → OpenAI-compatible
    /// (llama.cpp, LM Studio, vLLM); otherwise Ollama native.
    /// `SYNREPO_LLM_LOCAL_ENDPOINT` env var overrides this.
    #[serde(default)]
    pub local_endpoint: Option<String>,

    /// Informational preset label recorded at wizard time: `ollama`,
    /// `llamacpp`, `lmstudio`, `vllm`, or `custom`. `local_endpoint` is
    /// authoritative for dispatch; this field exists only so tooling can
    /// display a friendly name.
    #[serde(default)]
    pub local_preset: Option<String>,

    /// Maximum commentary provider calls to run at once during refresh.
    #[serde(default = "default_commentary_concurrency")]
    pub commentary_concurrency: usize,
}

impl Default for ExplainConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: None,
            anthropic_api_key: None,
            openai_api_key: None,
            gemini_api_key: None,
            openrouter_api_key: None,
            zai_api_key: None,
            minimax_api_key: None,
            model: None,
            local_endpoint: None,
            local_preset: None,
            commentary_concurrency: default_commentary_concurrency(),
        }
    }
}

impl ExplainConfig {
    /// Runtime commentary concurrency, clamped to at least one worker.
    pub fn commentary_concurrency(&self) -> usize {
        self.commentary_concurrency.max(1)
    }

    /// Merge another explain config into this one. `other` wins on all fields.
    pub fn merge(&mut self, other: Self) {
        if other.enabled {
            self.enabled = true;
        }
        if other.provider.is_some() {
            self.provider = other.provider;
        }
        if other.anthropic_api_key.is_some() {
            self.anthropic_api_key = other.anthropic_api_key;
        }
        if other.openai_api_key.is_some() {
            self.openai_api_key = other.openai_api_key;
        }
        if other.gemini_api_key.is_some() {
            self.gemini_api_key = other.gemini_api_key;
        }
        if other.openrouter_api_key.is_some() {
            self.openrouter_api_key = other.openrouter_api_key;
        }
        if other.zai_api_key.is_some() {
            self.zai_api_key = other.zai_api_key;
        }
        if other.minimax_api_key.is_some() {
            self.minimax_api_key = other.minimax_api_key;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.local_endpoint.is_some() {
            self.local_endpoint = other.local_endpoint;
        }
        if other.local_preset.is_some() {
            self.local_preset = other.local_preset;
        }
        if other.commentary_concurrency != default_commentary_concurrency() {
            self.commentary_concurrency = other.commentary_concurrency;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_block_loads_with_defaults() {
        // Config::load reads ~/.synrepo/config.toml; redirect HOME to an empty
        // tempdir under the shared lock so the developer's real user-scoped
        // credentials cannot leak defaults into these assertions.
        let _lock =
            crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());

        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();
        // A pre-explain-block config.toml.
        fs::write(synrepo_dir.join("config.toml"), "mode = \"auto\"\n").unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert!(!config.explain.enabled);
        assert!(config.explain.provider.is_none());
        assert!(config.explain.anthropic_api_key.is_none());
        assert!(config.explain.openai_api_key.is_none());
        assert!(config.explain.gemini_api_key.is_none());
        assert!(config.explain.openrouter_api_key.is_none());
        assert!(config.explain.zai_api_key.is_none());
        assert!(config.explain.minimax_api_key.is_none());
        assert!(config.explain.model.is_none());
        assert!(config.explain.local_endpoint.is_none());
        assert!(config.explain.local_preset.is_none());
        assert_eq!(config.explain.commentary_concurrency, 4);
        assert_eq!(config.explain.commentary_concurrency(), 4);
    }

    #[test]
    fn populated_block_round_trips() {
        let _lock =
            crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());

        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();
        let toml = r#"
            [explain]
            enabled = true
            provider = "local"
            openai_api_key = "sk-test"
            model = "llama3"
            local_endpoint = "http://localhost:11434/api/chat"
            local_preset = "ollama"
            commentary_concurrency = 8
        "#;
        fs::write(synrepo_dir.join("config.toml"), toml).unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert!(config.explain.enabled);
        assert_eq!(config.explain.provider.as_deref(), Some("local"));
        assert_eq!(config.explain.openai_api_key.as_deref(), Some("sk-test"));
        assert_eq!(config.explain.model.as_deref(), Some("llama3"));
        assert_eq!(
            config.explain.local_endpoint.as_deref(),
            Some("http://localhost:11434/api/chat")
        );
        assert_eq!(config.explain.local_preset.as_deref(), Some("ollama"));
        assert_eq!(config.explain.commentary_concurrency, 8);
    }

    #[test]
    fn partial_block_fills_unset_with_defaults() {
        let _lock =
            crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());

        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();
        let toml = r#"
            [explain]
            enabled = true
        "#;
        fs::write(synrepo_dir.join("config.toml"), toml).unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert!(config.explain.enabled);
        assert!(config.explain.provider.is_none());
        assert_eq!(config.explain.commentary_concurrency, 4);
    }

    #[test]
    fn legacy_synthesis_block_is_rejected() {
        let _lock =
            crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());

        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();
        fs::write(
            synrepo_dir.join("config.toml"),
            "[synthesis]\nenabled = true\n",
        )
        .unwrap();

        let error = Config::load(dir.path()).expect_err("legacy block should fail");
        assert!(
            error
                .to_string()
                .contains("legacy [synthesis]; rename it to [explain]"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn merge_explain_configs() {
        let mut base = ExplainConfig {
            enabled: false,
            provider: Some("anthropic".to_string()),
            anthropic_api_key: Some("global-key".to_string()),
            ..Default::default()
        };
        let other = ExplainConfig {
            enabled: true,
            openai_api_key: Some("local-key".to_string()),
            model: Some("gpt-4".to_string()),
            commentary_concurrency: 2,
            ..Default::default()
        };

        base.merge(other);

        assert!(base.enabled);
        assert_eq!(base.provider.as_deref(), Some("anthropic"));
        assert_eq!(base.anthropic_api_key.as_deref(), Some("global-key"));
        assert_eq!(base.openai_api_key.as_deref(), Some("local-key"));
        assert_eq!(base.model.as_deref(), Some("gpt-4"));
        assert_eq!(base.commentary_concurrency, 2);
    }

    #[test]
    fn commentary_concurrency_clamps_to_one() {
        let config = ExplainConfig {
            commentary_concurrency: 0,
            ..Default::default()
        };
        assert_eq!(config.commentary_concurrency(), 1);
    }
}
