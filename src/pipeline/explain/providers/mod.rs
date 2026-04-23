//! Explain provider abstraction layer.
//!
//! Supports multiple LLM providers: Anthropic, OpenAI, Gemini, OpenRouter,
//! Z.ai (Zhipu GLM), MiniMax, and local (Ollama/vLLM/llama.cpp/LM Studio).
//! Factory functions gate activation on `config.explain.enabled`
//! (persisted in `.synrepo/config.toml`) or the `SYNREPO_LLM_ENABLED=1`
//! one-shot env override. Without opt-in, explain is a no-op even if
//! provider API keys are present in the environment.
//!
//! Precedence for provider / model / local endpoint is: env var > config
//! value > compiled default. This lets persistent defaults live in the
//! config file while env vars remain the short-lived override.

pub mod anthropic;
/// Provider factory functions for building generators.
pub mod factories;
pub mod gemini;
pub mod http;
pub mod local;
pub mod minimax;
pub mod openai;
pub mod openai_compat;
pub mod openrouter;
/// Provider resolution types and configuration.
pub mod resolvers;
pub mod shared;
pub mod zai;

pub use factories::{
    build_commentary_generator, build_cross_link_generator, describe_active_provider,
    ActiveProvider, ExplainStatus,
};
pub use resolvers::{EndpointSource, ProviderConfig, ProviderKind};

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

    fn enabled_config() -> crate::config::Config {
        let mut c = crate::config::Config::default();
        c.explain.enabled = true;
        c
    }

    #[test]
    fn describe_default_config_without_keys_is_disabled() {
        let _env = EnvGuard::new();
        let active = describe_active_provider(&crate::config::Config::default());
        assert_eq!(active.provider, "anthropic");
        assert_eq!(active.status, ExplainStatus::Disabled);
    }

    #[test]
    fn describe_default_config_with_anthropic_key_hints_opt_in() {
        let env = EnvGuard::new();
        env.set("ANTHROPIC_API_KEY", "sk-test");
        let active = describe_active_provider(&crate::config::Config::default());
        assert!(matches!(
            active.status,
            ExplainStatus::DisabledKeyDetected {
                env_var: "ANTHROPIC_API_KEY"
            }
        ));
    }

    #[test]
    fn describe_enabled_config_reports_enabled() {
        let _env = EnvGuard::new();
        let active = describe_active_provider(&enabled_config());
        assert_eq!(active.status, ExplainStatus::Enabled);
    }

    #[test]
    fn env_enabled_flag_overrides_disabled_config() {
        let env = EnvGuard::new();
        env.set("SYNREPO_LLM_ENABLED", "1");
        let active = describe_active_provider(&crate::config::Config::default());
        assert_eq!(active.status, ExplainStatus::Enabled);
    }

    #[test]
    fn env_provider_wins_over_config_provider() {
        let env = EnvGuard::new();
        env.set("SYNREPO_LLM_PROVIDER", "openai");
        let mut config = enabled_config();
        config.explain.provider = Some("anthropic".to_string());
        let active = describe_active_provider(&config);
        assert_eq!(active.provider, "openai");
    }

    #[test]
    fn config_provider_used_when_env_unset() {
        let _env = EnvGuard::new();
        let mut config = enabled_config();
        config.explain.provider = Some("gemini".to_string());
        let active = describe_active_provider(&config);
        assert_eq!(active.provider, "gemini");
    }

    #[test]
    fn disabled_config_with_key_returns_noop_commentary_generator() {
        let env = EnvGuard::new();
        env.set("ANTHROPIC_API_KEY", "sk-test");
        // Build with disabled config; the generator MUST be NoOp even though
        // a key is present. This is the key-safety invariant.
        let gen = build_commentary_generator(&crate::config::Config::default(), 1024);
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
        config.explain.provider = Some("openai".to_string());
        config.explain.openai_api_key = Some("saved-openai-key".to_string());
        let resolved = ProviderConfig::resolve(ProviderKind::OpenAi, &config, 1024);
        assert_eq!(resolved.api_key.as_deref(), Some("saved-openai-key"));
    }

    #[test]
    fn env_key_wins_over_saved_config_key() {
        let env = EnvGuard::new();
        env.set("OPENAI_API_KEY", "env-openai-key");
        let mut config = enabled_config();
        config.explain.provider = Some("openai".to_string());
        config.explain.openai_api_key = Some("saved-openai-key".to_string());
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
        config.explain.provider = Some("local".to_string());
        config.explain.local_endpoint = Some("http://config-host:1111/api/chat".to_string());
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
        config.explain.provider = Some("local".to_string());
        config.explain.local_endpoint = Some("http://config-host:1111/api/chat".to_string());
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
        config.explain.provider = Some("local".to_string());
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
