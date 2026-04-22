//! Synthesis provider abstraction layer.
//!
//! Supports multiple LLM providers: Anthropic, OpenAI, Gemini, OpenRouter,
//! Z.ai (Zhipu GLM), MiniMax, and local (Ollama/vLLM/llama.cpp/LM Studio).
//! Factory functions gate activation on `config.synthesis.enabled`
//! (persisted in `.synrepo/config.toml`) or the `SYNREPO_LLM_ENABLED=1`
//! one-shot env override. Without opt-in, synthesis is a no-op even if
//! provider API keys are present in the environment.
//!
//! Precedence for provider / model / local endpoint is: env var > config
//! value > compiled default. This lets persistent defaults live in the
//! config file while env vars remain the short-lived override.

use crate::config::Config as SynrepoConfig;
use crate::overlay::ConfidenceThresholds;

use super::cross_link::CrossLinkGenerator;
use super::{CommentaryGenerator, NoOpCrossLinkGenerator, NoOpGenerator};

pub mod anthropic;
pub mod gemini;
pub mod http;
pub mod local;
pub mod minimax;
pub mod openai;
pub mod openai_compat;
pub mod openrouter;
pub mod shared;
pub mod zai;

/// Available synthesis providers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProviderKind {
    /// Anthropic (Claude) provider - default when no provider is configured.
    #[default]
    Anthropic,
    /// OpenAI provider (ChatGPT models)
    OpenAi,
    /// Google Gemini provider
    Gemini,
    /// OpenRouter provider
    OpenRouter,
    /// Z.ai (Zhipu GLM) provider
    Zai,
    /// MiniMax provider (international endpoint)
    Minimax,
    /// Local provider (Ollama, vLLM, llama.cpp, LM Studio)
    Local,
    /// Explicitly disabled - always returns NoOp generators
    None,
}

impl ProviderKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "anthropic" => Some(ProviderKind::Anthropic),
            "openai" => Some(ProviderKind::OpenAi),
            "gemini" => Some(ProviderKind::Gemini),
            "openrouter" => Some(ProviderKind::OpenRouter),
            "zai" => Some(ProviderKind::Zai),
            "minimax" => Some(ProviderKind::Minimax),
            "local" => Some(ProviderKind::Local),
            "none" => Some(ProviderKind::None),
            _ => None,
        }
    }

    /// Resolve the active provider. Env (`SYNREPO_LLM_PROVIDER`) wins over
    /// `config.synthesis.provider`; unknown values fall back to the default.
    pub fn resolve(config: &SynrepoConfig) -> Self {
        if let Ok(raw) = std::env::var("SYNREPO_LLM_PROVIDER") {
            if let Some(kind) = Self::parse(raw.trim()) {
                return kind;
            }
            tracing::warn!("unknown SYNREPO_LLM_PROVIDER '{raw}', ignoring");
        }
        if let Some(raw) = config.synthesis.provider.as_deref() {
            if let Some(kind) = Self::parse(raw.trim()) {
                return kind;
            }
            tracing::warn!("unknown synthesis.provider '{raw}' in config, ignoring");
        }
        ProviderKind::default()
    }

    /// Returns the display name for this provider.
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenAi => "openai",
            ProviderKind::Gemini => "gemini",
            ProviderKind::OpenRouter => "openrouter",
            ProviderKind::Zai => "zai",
            ProviderKind::Minimax => "minimax",
            ProviderKind::Local => "local",
            ProviderKind::None => "none",
        }
    }
}

/// Source of an endpoint or model configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointSource {
    /// Resolved from an environment variable (e.g. `SYNREPO_LLM_LOCAL_ENDPOINT`).
    Environment,
    /// Resolved from a configuration file (e.g. `.synrepo/config.toml`).
    Config,
    /// Using the compiled default value.
    Default,
}

impl EndpointSource {
    /// Returns a display label for the source.
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Config => "config",
            Self::Default => "default",
        }
    }
}

/// Resolved provider wiring: API key, model name, and (for Local) endpoint.
pub struct ProviderConfig {
    /// Provider-specific API key (unset for Local / None).
    pub api_key: Option<String>,
    /// Model name to use. Empty string means "use provider default".
    pub model: Option<String>,
    /// Local-provider endpoint (only meaningful when kind == Local).
    pub local_endpoint: Option<String>,
    /// Source of the resolved endpoint.
    pub endpoint_source: EndpointSource,
    /// Maximum tokens per API call (chars-based budget).
    pub max_tokens_per_call: u32,
}

impl ProviderConfig {
    /// Resolve API key, model, and endpoint for the selected provider,
    /// applying env-over-config-over-default precedence.
    pub fn resolve(
        provider: ProviderKind,
        config: &SynrepoConfig,
        max_tokens_per_call: u32,
    ) -> Self {
        let model_override = std::env::var("SYNREPO_LLM_MODEL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| config.synthesis.model.clone().filter(|s| !s.is_empty()));

        let (api_key, local_endpoint, endpoint_source) = match provider {
            ProviderKind::Anthropic => {
                let key = std::env::var("ANTHROPIC_API_KEY")
                    .or_else(|_| std::env::var("SYNREPO_ANTHROPIC_API_KEY"))
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .synthesis
                            .anthropic_api_key
                            .clone()
                            .filter(|k| !k.is_empty())
                    });
                (key, None, EndpointSource::Default)
            }
            ProviderKind::OpenAi => {
                let key = std::env::var("OPENAI_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .synthesis
                            .openai_api_key
                            .clone()
                            .filter(|k| !k.is_empty())
                    });
                (key, None, EndpointSource::Default)
            }
            ProviderKind::Gemini => {
                let key = std::env::var("GEMINI_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .synthesis
                            .gemini_api_key
                            .clone()
                            .filter(|k| !k.is_empty())
                    });
                (key, None, EndpointSource::Default)
            }
            ProviderKind::OpenRouter => {
                let key = std::env::var("OPENROUTER_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .synthesis
                            .openrouter_api_key
                            .clone()
                            .filter(|k| !k.is_empty())
                    });
                (key, None, EndpointSource::Default)
            }
            ProviderKind::Zai => {
                let key = std::env::var("ZAI_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .synthesis
                            .zai_api_key
                            .clone()
                            .filter(|k| !k.is_empty())
                    });
                (key, None, EndpointSource::Default)
            }
            ProviderKind::Minimax => {
                let key = std::env::var("MINIMAX_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .synthesis
                            .minimax_api_key
                            .clone()
                            .filter(|k| !k.is_empty())
                    });
                (key, None, EndpointSource::Default)
            }
            ProviderKind::Local => {
                if let Some(endpoint) = std::env::var("SYNREPO_LLM_LOCAL_ENDPOINT")
                    .ok()
                    .filter(|s| !s.is_empty())
                {
                    (None, Some(endpoint), EndpointSource::Environment)
                } else if let Some(endpoint) = config
                    .synthesis
                    .local_endpoint
                    .clone()
                    .filter(|s| !s.is_empty())
                {
                    (None, Some(endpoint), EndpointSource::Config)
                } else {
                    (None, None, EndpointSource::Default)
                }
            }
            ProviderKind::None => (None, None, EndpointSource::Default),
        };

        Self {
            api_key,
            model: model_override,
            local_endpoint,
            endpoint_source,
            max_tokens_per_call,
        }
    }
}

/// Returns true when synthesis is opted in, via either
/// `config.synthesis.enabled = true` or `SYNREPO_LLM_ENABLED=1`.
fn synthesis_opted_in(config: &SynrepoConfig) -> bool {
    if config.synthesis.enabled {
        return true;
    }
    matches!(std::env::var("SYNREPO_LLM_ENABLED").as_deref(), Ok("1"))
}

/// Build a commentary generator based on the active provider.
///
/// Returns a NoOp generator when synthesis is not opted in, when the
/// provider is `None`, or when a cloud provider is selected without its
/// API key present.
pub fn build_commentary_generator(
    config: &SynrepoConfig,
    max_tokens_per_call: u32,
) -> Box<dyn CommentaryGenerator> {
    if !synthesis_opted_in(config) {
        return Box::new(NoOpGenerator);
    }

    let provider = ProviderKind::resolve(config);
    let resolved = ProviderConfig::resolve(provider, config, max_tokens_per_call);

    match provider {
        ProviderKind::Anthropic => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| anthropic::DEFAULT_MODEL.to_string());
                tracing::debug!("synthesis: anthropic (model: {})", model);
                Box::new(anthropic::AnthropicCommentaryGenerator::new(
                    key,
                    model,
                    max_tokens_per_call,
                ))
            } else {
                Box::new(NoOpGenerator)
            }
        }
        ProviderKind::OpenAi => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| openai::DEFAULT_MODEL.to_string());
                tracing::debug!("synthesis: openai (model: {})", model);
                openai::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator)
            }
        }
        ProviderKind::Gemini => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| gemini::DEFAULT_MODEL.to_string());
                tracing::debug!("synthesis: gemini (model: {})", model);
                Box::new(gemini::GeminiCommentaryGenerator::new(
                    key,
                    model,
                    max_tokens_per_call,
                ))
            } else {
                Box::new(NoOpGenerator)
            }
        }
        ProviderKind::OpenRouter => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| openrouter::DEFAULT_MODEL.to_string());
                tracing::debug!("synthesis: openrouter (model: {})", model);
                openrouter::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator)
            }
        }
        ProviderKind::Zai => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| zai::DEFAULT_MODEL.to_string());
                tracing::debug!("synthesis: zai (model: {})", model);
                zai::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator)
            }
        }
        ProviderKind::Minimax => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| minimax::DEFAULT_MODEL.to_string());
                tracing::debug!("synthesis: minimax (model: {})", model);
                minimax::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator)
            }
        }
        ProviderKind::Local => {
            let model = resolved
                .model
                .unwrap_or_else(|| local::DEFAULT_MODEL.to_string());
            match resolved.local_endpoint {
                Some(endpoint) => {
                    tracing::debug!(
                        "synthesis: local (model: {}, endpoint: {})",
                        model,
                        endpoint
                    );
                    Box::new(local::LocalCommentaryGenerator::with_endpoint(
                        model,
                        max_tokens_per_call,
                        &endpoint,
                    ))
                }
                None => {
                    tracing::debug!("synthesis: local (model: {}, endpoint: default)", model);
                    Box::new(local::LocalCommentaryGenerator::new(
                        model,
                        max_tokens_per_call,
                    ))
                }
            }
        }
        ProviderKind::None => Box::new(NoOpGenerator),
    }
}

/// Build a cross-link generator based on the active provider.
///
/// Gating rules mirror [`build_commentary_generator`].
pub fn build_cross_link_generator(
    config: &SynrepoConfig,
    max_tokens_per_call: u32,
    thresholds: ConfidenceThresholds,
) -> Box<dyn CrossLinkGenerator> {
    if !synthesis_opted_in(config) {
        return Box::new(NoOpCrossLinkGenerator);
    }

    let provider = ProviderKind::resolve(config);
    let resolved = ProviderConfig::resolve(provider, config, max_tokens_per_call);

    match provider {
        ProviderKind::Anthropic => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| anthropic::DEFAULT_MODEL.to_string());
                Box::new(anthropic::AnthropicCrossLinkGenerator::new(
                    key,
                    model,
                    max_tokens_per_call,
                    thresholds,
                ))
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::OpenAi => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| openai::DEFAULT_MODEL.to_string());
                openai::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Gemini => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| gemini::DEFAULT_MODEL.to_string());
                Box::new(gemini::GeminiCrossLinkGenerator::new(
                    key,
                    model,
                    max_tokens_per_call,
                    thresholds,
                ))
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::OpenRouter => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| openrouter::DEFAULT_MODEL.to_string());
                openrouter::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Zai => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| zai::DEFAULT_MODEL.to_string());
                zai::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Minimax => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| minimax::DEFAULT_MODEL.to_string());
                minimax::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Local => {
            let model = resolved
                .model
                .unwrap_or_else(|| local::DEFAULT_MODEL.to_string());
            match resolved.local_endpoint {
                Some(endpoint) => Box::new(local::LocalCrossLinkGenerator::with_endpoint(
                    model,
                    max_tokens_per_call,
                    thresholds,
                    &endpoint,
                )),
                None => Box::new(local::LocalCrossLinkGenerator::new(
                    model,
                    max_tokens_per_call,
                    thresholds,
                )),
            }
        }
        ProviderKind::None => Box::new(NoOpCrossLinkGenerator),
    }
}

/// Enablement state of synthesis, for surface-layer display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SynthesisStatus {
    /// Synthesis is opted in and ready to call the provider.
    Enabled,
    /// Synthesis is disabled and at least one provider API key is present
    /// in the environment. Surfaces use this to hint the user that they
    /// can opt in without setting up credentials first.
    DisabledKeyDetected {
        /// Name of the env var that triggered the hint (e.g. `ANTHROPIC_API_KEY`).
        env_var: &'static str,
    },
    /// Synthesis is disabled and no provider API key is present. This is
    /// the expected default state.
    Disabled,
}

/// Rich description of the active provider for status / TUI display.
#[derive(Clone, Debug)]
pub struct ActiveProvider {
    /// Provider name (lowercase, stable).
    pub provider: &'static str,
    /// Default model for the provider (None for `ProviderKind::None`).
    pub model: Option<String>,
    /// Active local endpoint, if using ProviderKind::Local.
    pub local_endpoint: Option<String>,
    /// Source of the local endpoint.
    pub endpoint_source: EndpointSource,
    /// Whether synthesis will actually run with the current config+env.
    pub status: SynthesisStatus,
}

/// Describe the active provider given the resolved config and the current
/// environment. Does not perform network I/O. Used by `status` and the TUI.
pub fn describe_active_provider(config: &SynrepoConfig) -> ActiveProvider {
    let provider = ProviderKind::resolve(config);
    let (name, default_model) = match provider {
        ProviderKind::Anthropic => ("anthropic", Some(anthropic::DEFAULT_MODEL)),
        ProviderKind::OpenAi => ("openai", Some(openai::DEFAULT_MODEL)),
        ProviderKind::Gemini => ("gemini", Some(gemini::DEFAULT_MODEL)),
        ProviderKind::OpenRouter => ("openrouter", Some(openrouter::DEFAULT_MODEL)),
        ProviderKind::Zai => ("zai", Some(zai::DEFAULT_MODEL)),
        ProviderKind::Minimax => ("minimax", Some(minimax::DEFAULT_MODEL)),
        ProviderKind::Local => ("local", Some(local::DEFAULT_MODEL)),
        ProviderKind::None => ("none", None),
    };

    let resolved = ProviderConfig::resolve(provider, config, 1024);

    let model = resolved
        .model
        .or_else(|| default_model.map(|m| m.to_string()));

    let status = if synthesis_opted_in(config) {
        SynthesisStatus::Enabled
    } else if let Some(env_var) = detect_provider_key_env() {
        SynthesisStatus::DisabledKeyDetected { env_var }
    } else {
        SynthesisStatus::Disabled
    };

    ActiveProvider {
        provider: name,
        model,
        local_endpoint: resolved.local_endpoint,
        endpoint_source: resolved.endpoint_source,
        status,
    }
}

/// Return the first known provider-key env var that is set and non-empty,
/// in a stable order. Used to surface "key detected, synthesis disabled"
/// hints when the user has not opted in.
fn detect_provider_key_env() -> Option<&'static str> {
    const CANDIDATES: &[&str] = &[
        "ANTHROPIC_API_KEY",
        "SYNREPO_ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
        "OPENROUTER_API_KEY",
        "ZAI_API_KEY",
        "MINIMAX_API_KEY",
    ];
    for name in CANDIDATES {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

// Re-export for backward compatibility
pub use anthropic::{AnthropicCommentaryGenerator, AnthropicCrossLinkGenerator};
pub use gemini::{GeminiCommentaryGenerator, GeminiCrossLinkGenerator};
pub use local::{LocalCommentaryGenerator, LocalCrossLinkGenerator};
pub use minimax::{MinimaxCommentaryGenerator, MinimaxCrossLinkGenerator};
pub use openai::{OpenAiCommentaryGenerator, OpenAiCrossLinkGenerator};
pub use openrouter::{OpenRouterCommentaryGenerator, OpenRouterCrossLinkGenerator};
pub use zai::{ZaiCommentaryGenerator, ZaiCrossLinkGenerator};

// Legacy type aliases for compatibility
/// Alias for [`anthropic::AnthropicCommentaryGenerator`] - kept for backward compatibility.
#[allow(unused)]
pub type ClaudeCommentaryGenerator = anthropic::AnthropicCommentaryGenerator;
/// Alias for [`anthropic::AnthropicCrossLinkGenerator`] - kept for backward compatibility.
#[allow(unused)]
pub type ClaudeCrossLinkGenerator = anthropic::AnthropicCrossLinkGenerator;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env vars this module reads. Tests must clear them all before asserting
    // to avoid cross-test pollution; a single mutex serializes access to the
    // global process env.
    const RELEVANT_ENV: &[&str] = &[
        "SYNREPO_LLM_ENABLED",
        "SYNREPO_LLM_PROVIDER",
        "SYNREPO_LLM_MODEL",
        "SYNREPO_LLM_LOCAL_ENDPOINT",
        "ANTHROPIC_API_KEY",
        "SYNREPO_ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
        "OPENROUTER_API_KEY",
        "ZAI_API_KEY",
        "MINIMAX_API_KEY",
    ];

    // Global lock: env is process-wide shared state. Every test in this
    // module grabs the guard before touching env and releases only after its
    // assertions finish. Serializing at the test-module level is safer than
    // relying on `cargo test --test-threads=1`.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            for var in RELEVANT_ENV {
                std::env::remove_var(var);
            }
            Self { _guard: guard }
        }

        fn set(&self, key: &str, value: &str) {
            std::env::set_var(key, value);
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for var in RELEVANT_ENV {
                std::env::remove_var(var);
            }
        }
    }

    fn enabled_config() -> SynrepoConfig {
        let mut c = SynrepoConfig::default();
        c.synthesis.enabled = true;
        c
    }

    #[test]
    fn describe_default_config_without_keys_is_disabled() {
        let _env = EnvGuard::new();
        let active = describe_active_provider(&SynrepoConfig::default());
        assert_eq!(active.provider, "anthropic");
        assert_eq!(active.status, SynthesisStatus::Disabled);
    }

    #[test]
    fn describe_default_config_with_anthropic_key_hints_opt_in() {
        let env = EnvGuard::new();
        env.set("ANTHROPIC_API_KEY", "sk-test");
        let active = describe_active_provider(&SynrepoConfig::default());
        assert!(matches!(
            active.status,
            SynthesisStatus::DisabledKeyDetected {
                env_var: "ANTHROPIC_API_KEY"
            }
        ));
    }

    #[test]
    fn describe_enabled_config_reports_enabled() {
        let _env = EnvGuard::new();
        let active = describe_active_provider(&enabled_config());
        assert_eq!(active.status, SynthesisStatus::Enabled);
    }

    #[test]
    fn env_enabled_flag_overrides_disabled_config() {
        let env = EnvGuard::new();
        env.set("SYNREPO_LLM_ENABLED", "1");
        let active = describe_active_provider(&SynrepoConfig::default());
        assert_eq!(active.status, SynthesisStatus::Enabled);
    }

    #[test]
    fn env_provider_wins_over_config_provider() {
        let env = EnvGuard::new();
        env.set("SYNREPO_LLM_PROVIDER", "openai");
        let mut config = enabled_config();
        config.synthesis.provider = Some("anthropic".to_string());
        let active = describe_active_provider(&config);
        assert_eq!(active.provider, "openai");
    }

    #[test]
    fn config_provider_used_when_env_unset() {
        let _env = EnvGuard::new();
        let mut config = enabled_config();
        config.synthesis.provider = Some("gemini".to_string());
        let active = describe_active_provider(&config);
        assert_eq!(active.provider, "gemini");
    }

    #[test]
    fn disabled_config_with_key_returns_noop_commentary_generator() {
        let env = EnvGuard::new();
        env.set("ANTHROPIC_API_KEY", "sk-test");
        // Build with disabled config; the generator MUST be NoOp even though
        // a key is present. This is the key-safety invariant.
        let gen = build_commentary_generator(&SynrepoConfig::default(), 1024);
        // NoOp always returns Ok(None) for any call; we detect it by
        // checking that generate() returns None even with non-empty context.
        use crate::core::ids::{FileNodeId, NodeId};
        let node = NodeId::File(FileNodeId(1));
        assert!(gen.generate(node, "some context").unwrap().is_none());
    }

    #[test]
    fn config_key_used_when_env_unset() {
        let _env = EnvGuard::new();
        let mut config = enabled_config();
        config.synthesis.provider = Some("openai".to_string());
        config.synthesis.openai_api_key = Some("saved-openai-key".to_string());
        let resolved = ProviderConfig::resolve(ProviderKind::OpenAi, &config, 1024);
        assert_eq!(resolved.api_key.as_deref(), Some("saved-openai-key"));
    }

    #[test]
    fn env_key_wins_over_saved_config_key() {
        let env = EnvGuard::new();
        env.set("OPENAI_API_KEY", "env-openai-key");
        let mut config = enabled_config();
        config.synthesis.provider = Some("openai".to_string());
        config.synthesis.openai_api_key = Some("saved-openai-key".to_string());
        let resolved = ProviderConfig::resolve(ProviderKind::OpenAi, &config, 1024);
        assert_eq!(resolved.api_key.as_deref(), Some("env-openai-key"));
    }

    #[test]
    fn local_endpoint_env_wins_over_config() {
        let env = EnvGuard::new();
        env.set(
            "SYNREPO_LLM_LOCAL_ENDPOINT",
            "http://env-host:9999/api/chat",
        );
        let mut config = enabled_config();
        config.synthesis.provider = Some("local".to_string());
        config.synthesis.local_endpoint = Some("http://config-host:1111/api/chat".to_string());
        let resolved = ProviderConfig::resolve(ProviderKind::Local, &config, 1024);
        assert_eq!(
            resolved.local_endpoint.as_deref(),
            Some("http://env-host:9999/api/chat")
        );
    }

    #[test]
    fn local_endpoint_config_used_when_env_unset() {
        let _env = EnvGuard::new();
        let mut config = enabled_config();
        config.synthesis.provider = Some("local".to_string());
        config.synthesis.local_endpoint = Some("http://config-host:1111/api/chat".to_string());
        let resolved = ProviderConfig::resolve(ProviderKind::Local, &config, 1024);
        assert_eq!(
            resolved.local_endpoint.as_deref(),
            Some("http://config-host:1111/api/chat")
        );
    }

    #[test]
    fn local_endpoint_defaults_to_ollama_when_unset() {
        let _env = EnvGuard::new();
        let mut config = enabled_config();
        config.synthesis.provider = Some("local".to_string());
        // No local_endpoint set in config or env.
        let resolved = ProviderConfig::resolve(ProviderKind::Local, &config, 1024);
        // It should be None here, and the builder should use the default.
        assert!(resolved.local_endpoint.is_none());

        let _gen = build_commentary_generator(&config, 1024);
        // We can't easily inspect the inner endpoint of the box, but we can verify it doesn't panic
        // and uses the expected path in describe_active_provider.
        let active = describe_active_provider(&config);
        assert_eq!(active.provider, "local");
        assert!(active.local_endpoint.is_none());
    }
}
