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

/// Date the rate table was last verified against public pricing pages.
///
/// Surfaces echo this string in the Health tab so the user can judge
/// freshness. Format is ISO-8601 `YYYY-MM-DD`.
pub const LAST_UPDATED: &str = "2026-04-19";

/// Entry in the pricing table.
#[derive(Clone, Copy, Debug)]
struct Rate {
    provider: &'static str,
    model: &'static str,
    input_per_1m_usd: f64,
    output_per_1m_usd: f64,
}

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
        input_per_1m_usd: 15.0,
        output_per_1m_usd: 75.0,
    },
    Rate {
        provider: "anthropic",
        model: "claude-haiku-4-5-20251001",
        input_per_1m_usd: 0.80,
        output_per_1m_usd: 4.0,
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
    let rate = RATES
        .iter()
        .find(|r| r.provider == provider && r.model == model)?;
    let input_cost = (input_tokens as f64) * rate.input_per_1m_usd / 1_000_000.0;
    let output_cost = (output_tokens as f64) * rate.output_per_1m_usd / 1_000_000.0;
    Some(input_cost + output_cost)
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
}
