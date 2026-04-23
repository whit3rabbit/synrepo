use crate::config::Config as SynrepoConfig;

/// Available explain providers.
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
    pub(crate) fn parse(raw: &str) -> Option<Self> {
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
    /// `config.explain.provider`; unknown values fall back to the default.
    pub fn resolve(config: &SynrepoConfig) -> Self {
        if let Ok(raw) = std::env::var("SYNREPO_LLM_PROVIDER") {
            if let Some(kind) = Self::parse(raw.trim()) {
                return kind;
            }
            tracing::warn!("unknown SYNREPO_LLM_PROVIDER '{raw}', ignoring");
        }
        if let Some(raw) = config.explain.provider.as_deref() {
            if let Some(kind) = Self::parse(raw.trim()) {
                return kind;
            }
            tracing::warn!("unknown explain.provider '{raw}' in config, ignoring");
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
            .or_else(|| config.explain.model.clone().filter(|s| !s.is_empty()));

        let (api_key, local_endpoint, endpoint_source) = match provider {
            ProviderKind::Anthropic => {
                let key = std::env::var("ANTHROPIC_API_KEY")
                    .or_else(|_| std::env::var("SYNREPO_ANTHROPIC_API_KEY"))
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .explain
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
                            .explain
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
                            .explain
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
                            .explain
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
                    .or_else(|| config.explain.zai_api_key.clone().filter(|k| !k.is_empty()));
                (key, None, EndpointSource::Default)
            }
            ProviderKind::Minimax => {
                let key = std::env::var("MINIMAX_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
                    .or_else(|| {
                        config
                            .explain
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
                    .explain
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

/// Returns true when explain is opted in, via either
/// `config.explain.enabled = true` or `SYNREPO_LLM_ENABLED=1`.
pub(crate) fn explain_opted_in(config: &SynrepoConfig) -> bool {
    if config.explain.enabled {
        return true;
    }
    matches!(std::env::var("SYNREPO_LLM_ENABLED").as_deref(), Ok("1"))
}
