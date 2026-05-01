use crate::config::Config as SynrepoConfig;
use crate::overlay::ConfidenceThresholds;
use crate::pipeline::explain::cross_link::CrossLinkGenerator;
use crate::pipeline::explain::{CommentaryGenerator, NoOpCrossLinkGenerator, NoOpGenerator};

use super::resolvers::{explain_opted_in, ProviderConfig, ProviderKind};

/// Build a commentary generator based on the active provider.
///
/// Returns a NoOp generator when explain is not opted in, when the
/// provider is `None`, or when a cloud provider is selected without its
/// API key present.
pub fn build_commentary_generator(
    config: &SynrepoConfig,
    max_tokens_per_call: u32,
) -> Box<dyn CommentaryGenerator> {
    if !explain_opted_in(config) {
        return Box::new(NoOpGenerator::provider_disabled());
    }

    let provider = ProviderKind::resolve(config);
    let resolved = ProviderConfig::resolve(provider, config, max_tokens_per_call);

    match provider {
        ProviderKind::Anthropic => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::anthropic::DEFAULT_MODEL.to_string());
                tracing::debug!("explain: anthropic (model: {})", model);
                Box::new(super::anthropic::AnthropicCommentaryGenerator::new(
                    key,
                    model,
                    max_tokens_per_call,
                ))
            } else {
                Box::new(NoOpGenerator::missing_api_key())
            }
        }
        ProviderKind::OpenAi => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::openai::DEFAULT_MODEL.to_string());
                tracing::debug!("explain: openai (model: {})", model);
                super::openai::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator::missing_api_key())
            }
        }
        ProviderKind::Gemini => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::gemini::DEFAULT_MODEL.to_string());
                tracing::debug!("explain: gemini (model: {})", model);
                Box::new(super::gemini::GeminiCommentaryGenerator::new(
                    key,
                    model,
                    max_tokens_per_call,
                ))
            } else {
                Box::new(NoOpGenerator::missing_api_key())
            }
        }
        ProviderKind::OpenRouter => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::openrouter::DEFAULT_MODEL.to_string());
                tracing::debug!("explain: openrouter (model: {})", model);
                super::openrouter::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator::missing_api_key())
            }
        }
        ProviderKind::Zai => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::zai::DEFAULT_MODEL.to_string());
                tracing::debug!("explain: zai (model: {})", model);
                super::zai::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator::missing_api_key())
            }
        }
        ProviderKind::Minimax => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::minimax::DEFAULT_MODEL.to_string());
                tracing::debug!("explain: minimax (model: {})", model);
                super::minimax::new_commentary(key, model, max_tokens_per_call)
            } else {
                Box::new(NoOpGenerator::missing_api_key())
            }
        }
        ProviderKind::Local => {
            let model = resolved
                .model
                .unwrap_or_else(|| super::local::DEFAULT_MODEL.to_string());
            match resolved.local_endpoint {
                Some(endpoint) => {
                    tracing::debug!("explain: local (model: {}, endpoint: {})", model, endpoint);
                    Box::new(super::local::LocalCommentaryGenerator::with_endpoint(
                        model,
                        max_tokens_per_call,
                        &endpoint,
                    ))
                }
                None => {
                    tracing::debug!("explain: local (model: {}, endpoint: default)", model);
                    Box::new(super::local::LocalCommentaryGenerator::new(
                        model,
                        max_tokens_per_call,
                    ))
                }
            }
        }
        ProviderKind::None => Box::new(NoOpGenerator::provider_disabled()),
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
    if !explain_opted_in(config) {
        return Box::new(NoOpCrossLinkGenerator);
    }

    let provider = ProviderKind::resolve(config);
    let resolved = ProviderConfig::resolve(provider, config, max_tokens_per_call);

    match provider {
        ProviderKind::Anthropic => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::anthropic::DEFAULT_MODEL.to_string());
                Box::new(super::anthropic::AnthropicCrossLinkGenerator::new(
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
                    .unwrap_or_else(|| super::openai::DEFAULT_MODEL.to_string());
                super::openai::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Gemini => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::gemini::DEFAULT_MODEL.to_string());
                Box::new(super::gemini::GeminiCrossLinkGenerator::new(
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
                    .unwrap_or_else(|| super::openrouter::DEFAULT_MODEL.to_string());
                super::openrouter::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Zai => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::zai::DEFAULT_MODEL.to_string());
                super::zai::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Minimax => {
            if let Some(key) = resolved.api_key {
                let model = resolved
                    .model
                    .unwrap_or_else(|| super::minimax::DEFAULT_MODEL.to_string());
                super::minimax::new_cross_link(key, model, max_tokens_per_call, thresholds)
            } else {
                Box::new(NoOpCrossLinkGenerator)
            }
        }
        ProviderKind::Local => {
            let model = resolved
                .model
                .unwrap_or_else(|| super::local::DEFAULT_MODEL.to_string());
            match resolved.local_endpoint {
                Some(endpoint) => Box::new(super::local::LocalCrossLinkGenerator::with_endpoint(
                    model,
                    max_tokens_per_call,
                    thresholds,
                    &endpoint,
                )),
                None => Box::new(super::local::LocalCrossLinkGenerator::new(
                    model,
                    max_tokens_per_call,
                    thresholds,
                )),
            }
        }
        ProviderKind::None => Box::new(NoOpCrossLinkGenerator),
    }
}

/// Enablement state of explain, for surface-layer display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplainStatus {
    /// Explain is opted in and ready to call the provider.
    Enabled,
    /// Explain is disabled and at least one provider API key is present
    /// in the environment. Surfaces use this to hint the user that they
    /// can opt in without setting up credentials first.
    DisabledKeyDetected {
        /// Name of the env var that triggered the hint (e.g. `ANTHROPIC_API_KEY`).
        env_var: &'static str,
    },
    /// Explain is disabled and no provider API key is present. This is
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
    pub endpoint_source: super::resolvers::EndpointSource,
    /// Whether explain will actually run with the current config+env.
    pub status: ExplainStatus,
}

/// Describe the active provider given the resolved config and the current
/// environment. Does not perform network I/O. Used by `status` and the TUI.
pub fn describe_active_provider(config: &SynrepoConfig) -> ActiveProvider {
    let provider = ProviderKind::resolve(config);
    let (name, default_model) = match provider {
        ProviderKind::Anthropic => ("anthropic", Some(super::anthropic::DEFAULT_MODEL)),
        ProviderKind::OpenAi => ("openai", Some(super::openai::DEFAULT_MODEL)),
        ProviderKind::Gemini => ("gemini", Some(super::gemini::DEFAULT_MODEL)),
        ProviderKind::OpenRouter => ("openrouter", Some(super::openrouter::DEFAULT_MODEL)),
        ProviderKind::Zai => ("zai", Some(super::zai::DEFAULT_MODEL)),
        ProviderKind::Minimax => ("minimax", Some(super::minimax::DEFAULT_MODEL)),
        ProviderKind::Local => ("local", Some(super::local::DEFAULT_MODEL)),
        ProviderKind::None => ("none", None),
    };

    let resolved = ProviderConfig::resolve(provider, config, 1024);

    let model = resolved
        .model
        .or_else(|| default_model.map(|m| m.to_string()));

    let status = if explain_opted_in(config) {
        ExplainStatus::Enabled
    } else if let Some(env_var) = detect_provider_key_env() {
        ExplainStatus::DisabledKeyDetected { env_var }
    } else {
        ExplainStatus::Disabled
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
/// in a stable order. Used to surface "key detected, explain disabled"
/// hints when the user has not opted in.
pub(crate) fn detect_provider_key_env() -> Option<&'static str> {
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
