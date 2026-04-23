//! Cloud and local provider type definitions for the setup wizard.

use crate::config::ExplainConfig;

/// Cloud explain providers offered by the wizard. Maps 1:1 to
/// `config.explain.provider` string values.
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
    /// Config string written under `[explain] provider = "<this>"`.
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

    /// Key written into `[explain]` when setup persists a reusable key in
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

    pub(crate) fn saved_key<'a>(&self, config: &'a ExplainConfig) -> Option<&'a str> {
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
    /// Rendered on the `ExplainExplain` step and on the review screen.
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
    /// Stable preset id for `config.explain.local_preset`.
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
