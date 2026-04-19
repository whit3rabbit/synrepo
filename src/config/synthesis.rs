//! Synthesis pipeline configuration.
//!
//! Off by default. Opting in via `enabled = true` is required even when a
//! provider API key is present in the environment. Without this safeguard,
//! synrepo would silently consume any `ANTHROPIC_API_KEY` /
//! `OPENAI_API_KEY` / `GEMINI_API_KEY` the user happens to have set for
//! unrelated tools.

use serde::{Deserialize, Serialize};

/// Synthesis provider configuration block persisted as `[synthesis]` in
/// `.synrepo/config.toml`. All fields are optional with serde defaults so
/// older config files load unchanged; missing or absent block means
/// "disabled, no preferences set".
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SynthesisConfig {
    /// When false, synthesis is disabled regardless of env keys. Set
    /// `SYNREPO_LLM_ENABLED=1` as a one-shot env override without persisting.
    #[serde(default)]
    pub enabled: bool,

    /// Provider selector: `anthropic | openai | gemini | local | none`.
    /// `SYNREPO_LLM_PROVIDER` env var overrides this when set.
    #[serde(default)]
    pub provider: Option<String>,

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
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_block_loads_with_defaults() {
        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();
        // A pre-synthesis-block config.toml.
        fs::write(synrepo_dir.join("config.toml"), "mode = \"auto\"\n").unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert!(!config.synthesis.enabled);
        assert!(config.synthesis.provider.is_none());
        assert!(config.synthesis.model.is_none());
        assert!(config.synthesis.local_endpoint.is_none());
        assert!(config.synthesis.local_preset.is_none());
    }

    #[test]
    fn populated_block_round_trips() {
        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();
        let toml = r#"
            [synthesis]
            enabled = true
            provider = "local"
            model = "llama3"
            local_endpoint = "http://localhost:11434/api/chat"
            local_preset = "ollama"
        "#;
        fs::write(synrepo_dir.join("config.toml"), toml).unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert!(config.synthesis.enabled);
        assert_eq!(config.synthesis.provider.as_deref(), Some("local"));
        assert_eq!(config.synthesis.model.as_deref(), Some("llama3"));
        assert_eq!(
            config.synthesis.local_endpoint.as_deref(),
            Some("http://localhost:11434/api/chat")
        );
        assert_eq!(config.synthesis.local_preset.as_deref(), Some("ollama"));
    }

    #[test]
    fn partial_block_fills_unset_with_defaults() {
        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();
        let toml = r#"
            [synthesis]
            enabled = true
        "#;
        fs::write(synrepo_dir.join("config.toml"), toml).unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert!(config.synthesis.enabled);
        assert!(config.synthesis.provider.is_none());
    }
}
