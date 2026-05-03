//! Context accounting metadata attached to card-shaped responses.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Budget;

/// Context accounting shared by card-shaped responses.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContextAccounting {
    /// Budget tier used to compile the response.
    pub budget_tier: Budget,
    /// Estimated response tokens.
    pub token_estimate: usize,
    /// Estimated tokens an agent would spend reading the raw source file(s).
    pub raw_file_token_estimate: usize,
    /// Estimated savings ratio in the range 0.0..=1.0 when raw tokens are known.
    pub estimated_savings_ratio: f64,
    /// Source content hashes used to build the response.
    pub source_hashes: Vec<String>,
    /// Whether the response includes stale advisory content.
    pub stale: bool,
    /// Whether content was omitted to satisfy a numeric token cap.
    pub truncation_applied: bool,
}

impl ContextAccounting {
    /// Build accounting metadata for a graph-backed card.
    pub fn new(
        budget_tier: Budget,
        token_estimate: usize,
        raw_file_token_estimate: usize,
        source_hashes: Vec<String>,
    ) -> Self {
        let estimated_savings_ratio = if raw_file_token_estimate > 0 {
            raw_file_token_estimate.saturating_sub(token_estimate) as f64
                / raw_file_token_estimate as f64
        } else {
            0.0
        };

        Self {
            budget_tier,
            token_estimate,
            raw_file_token_estimate,
            estimated_savings_ratio,
            source_hashes,
            stale: false,
            truncation_applied: false,
        }
    }

    /// Placeholder used before a card's final token estimate is computed.
    pub fn placeholder(budget_tier: Budget) -> Self {
        Self {
            budget_tier,
            ..Self::default()
        }
    }

    /// Mark the response as truncated by a numeric cap.
    pub fn with_truncation(mut self, truncation_applied: bool) -> Self {
        self.truncation_applied = truncation_applied;
        self
    }
}

/// Estimate tokens using a conservative 3-bytes-per-token approximation.
pub fn estimate_tokens_bytes(byte_len: usize) -> usize {
    (byte_len / 3).max(1)
}

/// Estimate raw source tokens for a repo-relative path.
pub fn raw_file_token_estimate(repo_root: Option<&Path>, repo_relative_path: &str) -> usize {
    let Some(repo_root) = repo_root else {
        return 0;
    };
    // Stat is enough for a byte-length estimate; avoid reading the file
    // body on every card compile just to get its length.
    fs::metadata(repo_root.join(repo_relative_path))
        .map(|meta| estimate_tokens_bytes(meta.len() as usize))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_estimator_uses_conservative_ratio() {
        assert_eq!(estimate_tokens_bytes(6), 2);
        assert_eq!(estimate_tokens_bytes(1), 1);
    }
}
