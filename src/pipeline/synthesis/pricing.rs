//! Static per-model USD rate table for synthesis accounting.
//!
//! Rates are quoted in USD per 1 million tokens, split into input
//! (prompt) and output (completion) dimensions. The table is a best-effort
//! snapshot; when a `(provider, model)` pair is not present,
//! [`cost_for_call`] returns `None` and the accounting layer records a
//! `null` USD cost rather than guessing. The [`LAST_UPDATED`] date is
//! surfaced verbatim to the user so they know how stale the number is.
//!
//! Do not "fill in" unknown entries with similar models. The whole point of
//! the `None` path is that we refuse to invent a number.

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::Deserialize;

use super::providers::http::{build_client, get_json_strict};

/// Date the rate table was last verified against public pricing pages.
///
/// Surfaces echo this string in the Health tab so the user can judge
/// freshness. Format is ISO-8601 `YYYY-MM-DD`.
pub const LAST_UPDATED: &str = "2026-04-19";
const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// Entry in the pricing table.
#[derive(Clone, Copy, Debug)]
struct Rate {
    provider: &'static str,
    model: &'static str,
    input_per_1m_usd: f64,
    output_per_1m_usd: f64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct OpenRouterRate {
    prompt_per_token_usd: OrderedF64,
    completion_per_token_usd: OrderedF64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct OrderedF64(u64);

impl OrderedF64 {
    fn new(value: f64) -> Self {
        Self(value.to_bits())
    }

    fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

static OPENROUTER_RATE_CACHE: OnceLock<Mutex<Option<HashMap<String, OpenRouterRate>>>> =
    OnceLock::new();

/// Known rates. Lookups are exact on `(provider, model)`; no fuzzy match.
const RATES: &[Rate] = &[
    // Anthropic — published pricing (per 1M tokens).
    Rate {
        provider: "anthropic",
        model: "claude-sonnet-4-6",
        input_per_1m_usd: 3.0,
        output_per_1m_usd: 15.0,
    },
    Rate {
        provider: "anthropic",
        model: "claude-opus-4-7",
        input_per_1m_usd: 5.0,
        output_per_1m_usd: 25.0,
    },
    Rate {
        provider: "anthropic",
        model: "claude-haiku-4-5-20251001",
        input_per_1m_usd: 1.0,
        output_per_1m_usd: 5.0,
    },
    // OpenAI
    Rate {
        provider: "openai",
        model: "gpt-4o-mini",
        input_per_1m_usd: 0.15,
        output_per_1m_usd: 0.60,
    },
    Rate {
        provider: "openai",
        model: "gpt-4o",
        input_per_1m_usd: 2.50,
        output_per_1m_usd: 10.0,
    },
    // Gemini
    Rate {
        provider: "gemini",
        model: "gemini-1.5-flash",
        input_per_1m_usd: 0.075,
        output_per_1m_usd: 0.30,
    },
    Rate {
        provider: "gemini",
        model: "gemini-1.5-pro",
        input_per_1m_usd: 1.25,
        output_per_1m_usd: 5.0,
    },
    // Local models always cost $0 from the user's wallet; record it so the
    // Health tab shows $0 instead of "unknown", which would be misleading.
    Rate {
        provider: "local",
        model: "",
        input_per_1m_usd: 0.0,
        output_per_1m_usd: 0.0,
    },
];

/// Compute USD cost for an API call given input/output token counts.
///
/// Returns `None` when `(provider, model)` is not in the rate table. Local
/// provider calls ignore `model` and always cost `Some(0.0)`.
pub fn cost_for_call(
    provider: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
) -> Option<f64> {
    if provider == "local" {
        return Some(0.0);
    }
    if provider == "openrouter" {
        let rate = openrouter_rate(model)?;
        return Some(
            (input_tokens as f64) * rate.prompt_per_token_usd.get()
                + (output_tokens as f64) * rate.completion_per_token_usd.get(),
        );
    }
    let rate = RATES
        .iter()
        .find(|r| r.provider == provider && r.model == model)?;
    let input_cost = (input_tokens as f64) * rate.input_per_1m_usd / 1_000_000.0;
    let output_cost = (output_tokens as f64) * rate.output_per_1m_usd / 1_000_000.0;
    Some(input_cost + output_cost)
}

/// Human-readable description of the pricing basis used by the current totals.
pub fn pricing_basis_label(openrouter_live: bool) -> String {
    if openrouter_live {
        format!("static table as of {LAST_UPDATED}; OpenRouter live")
    } else {
        format!("static table as of {LAST_UPDATED}")
    }
}

fn openrouter_rate(model: &str) -> Option<OpenRouterRate> {
    let cache = OPENROUTER_RATE_CACHE.get_or_init(|| Mutex::new(None));
    if let Some(rates) = cache.lock().as_ref() {
        return rates.get(model).copied();
    }

    let fetched = fetch_openrouter_rates().unwrap_or_default();
    let rate = fetched.get(model).copied();
    *cache.lock() = Some(fetched);
    rate
}

fn fetch_openrouter_rates() -> Option<HashMap<String, OpenRouterRate>> {
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|value| !value.is_empty())?;
    let auth_header = format!("Bearer {api_key}");
    let headers = [("Authorization", auth_header.as_str())];
    let parsed: OpenRouterModelsResponse =
        get_json_strict(&build_client(), OPENROUTER_MODELS_URL, &headers).ok()?;
    Some(parse_openrouter_rates(parsed))
}

fn parse_openrouter_rates(response: OpenRouterModelsResponse) -> HashMap<String, OpenRouterRate> {
    response
        .data
        .into_iter()
        .filter_map(|model| {
            Some((
                model.id,
                OpenRouterRate {
                    prompt_per_token_usd: OrderedF64::new(model.pricing.prompt.parse().ok()?),
                    completion_per_token_usd: OrderedF64::new(
                        model.pricing.completion.parse().ok()?,
                    ),
                },
            ))
        })
        .collect()
}

#[derive(Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Deserialize)]
struct OpenRouterModel {
    id: String,
    pricing: OpenRouterPricing,
}

#[derive(Deserialize)]
struct OpenRouterPricing {
    prompt: String,
    completion: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_pair_returns_cost() {
        // 1M input + 1M output on gpt-4o-mini = 0.15 + 0.60 = 0.75 USD.
        let cost = cost_for_call("openai", "gpt-4o-mini", 1_000_000, 1_000_000).unwrap();
        assert!((cost - 0.75).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn unknown_pair_returns_none() {
        assert!(cost_for_call("openai", "nonexistent-model", 100, 100).is_none());
    }

    #[test]
    fn local_always_zero() {
        assert_eq!(
            cost_for_call("local", "anything", 999_999, 999_999),
            Some(0.0)
        );
    }

    #[test]
    fn zero_tokens_zero_cost() {
        assert_eq!(
            cost_for_call("anthropic", "claude-sonnet-4-6", 0, 0),
            Some(0.0)
        );
    }

    #[test]
    fn parse_openrouter_rates_reads_per_token_pricing() {
        let rates = parse_openrouter_rates(OpenRouterModelsResponse {
            data: vec![OpenRouterModel {
                id: "openai/gpt-4".to_string(),
                pricing: OpenRouterPricing {
                    prompt: "0.00003".to_string(),
                    completion: "0.00006".to_string(),
                },
            }],
        });
        let rate = rates.get("openai/gpt-4").copied().expect("rate");
        assert!((rate.prompt_per_token_usd.get() - 0.00003).abs() < 1e-9);
        assert!((rate.completion_per_token_usd.get() - 0.00006).abs() < 1e-9);
    }
}
