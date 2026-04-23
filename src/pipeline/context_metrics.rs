//! Best-effort operational metrics for context-serving behavior.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::surface::card::{Budget, ContextAccounting};

const METRICS_FILE: &str = "context-metrics.json";

/// Aggregated context-serving metrics stored under `.synrepo/state/`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContextMetrics {
    /// Number of card-shaped responses served.
    pub cards_served_total: u64,
    /// Sum of estimated card tokens.
    pub card_tokens_total: u64,
    /// Sum of estimated raw-file tokens.
    pub raw_file_tokens_total: u64,
    /// Sum of estimated saved raw-file tokens.
    pub estimated_tokens_saved_total: u64,
    /// Count of responses by budget tier.
    pub budget_tier_usage: BTreeMap<String, u64>,
    /// Number of responses that report truncation.
    pub truncation_applied_total: u64,
    /// Number of responses that surfaced stale advisory content.
    pub stale_responses_total: u64,
    /// Number of test-surface responses with at least one discovered test.
    pub test_surface_hits_total: u64,
    /// Number of changed files observed by `synrepo_changed`.
    pub changed_files_total: u64,
    /// Total request latency recorded by card handlers.
    pub context_query_latency_ms_total: u64,
    /// Number of request latency samples.
    pub context_query_latency_samples: u64,
}

impl ContextMetrics {
    /// Average estimated card tokens per served card.
    pub fn card_tokens_avg(&self) -> f64 {
        if self.cards_served_total == 0 {
            0.0
        } else {
            self.card_tokens_total as f64 / self.cards_served_total as f64
        }
    }

    /// Average recorded context query latency.
    pub fn context_query_latency_ms_avg(&self) -> f64 {
        if self.context_query_latency_samples == 0 {
            0.0
        } else {
            self.context_query_latency_ms_total as f64 / self.context_query_latency_samples as f64
        }
    }

    /// Record a card-shaped response.
    pub fn record_card(&mut self, accounting: &ContextAccounting, latency_ms: u64) {
        self.cards_served_total += 1;
        self.card_tokens_total += accounting.token_estimate as u64;
        self.raw_file_tokens_total += accounting.raw_file_token_estimate as u64;
        self.estimated_tokens_saved_total += accounting
            .raw_file_token_estimate
            .saturating_sub(accounting.token_estimate)
            as u64;
        *self
            .budget_tier_usage
            .entry(budget_label(accounting.budget_tier).to_string())
            .or_default() += 1;
        if accounting.truncation_applied {
            self.truncation_applied_total += 1;
        }
        if accounting.stale {
            self.stale_responses_total += 1;
        }
        self.context_query_latency_ms_total += latency_ms;
        self.context_query_latency_samples += 1;
    }

    /// Record a test-surface hit.
    pub fn record_test_surface_hit(&mut self) {
        self.test_surface_hits_total += 1;
    }

    /// Record changed files observed by the changed-context surface.
    pub fn record_changed_files(&mut self, count: usize) {
        self.changed_files_total += count as u64;
    }
}

/// Load context metrics. Missing files return empty metrics.
pub fn load(synrepo_dir: &Path) -> anyhow::Result<ContextMetrics> {
    let path = metrics_path(synrepo_dir);
    if !path.exists() {
        return Ok(ContextMetrics::default());
    }
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Save context metrics.
pub fn save(synrepo_dir: &Path, metrics: &ContextMetrics) -> anyhow::Result<()> {
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir)?;
    let bytes = serde_json::to_vec_pretty(metrics)?;
    crate::util::atomic_write::atomic_write(&metrics_path(synrepo_dir), &bytes)?;
    Ok(())
}

/// Best-effort card metric recording. Failures are debug-only.
pub fn record_card_best_effort(
    synrepo_dir: &Path,
    accounting: &ContextAccounting,
    latency_ms: u64,
    test_surface_hit: bool,
) {
    record_cards_best_effort(
        synrepo_dir,
        std::slice::from_ref(accounting),
        latency_ms,
        test_surface_hit,
    );
}

/// Batched variant that loads and saves the metrics file once for a whole
/// response. The same latency is attributed to every card in the batch.
pub fn record_cards_best_effort(
    synrepo_dir: &Path,
    accountings: &[ContextAccounting],
    latency_ms: u64,
    test_surface_hit: bool,
) {
    if accountings.is_empty() {
        return;
    }
    if let Err(error) = (|| -> anyhow::Result<()> {
        let mut metrics = load(synrepo_dir)?;
        for accounting in accountings {
            metrics.record_card(accounting, latency_ms);
        }
        if test_surface_hit {
            metrics.record_test_surface_hit();
        }
        save(synrepo_dir, &metrics)
    })() {
        tracing::debug!(%error, "context metrics record failed");
    }
}

/// Best-effort changed-file metric recording.
pub fn record_changed_files_best_effort(synrepo_dir: &Path, count: usize) {
    if count == 0 {
        return;
    }
    if let Err(error) = (|| -> anyhow::Result<()> {
        let mut metrics = load(synrepo_dir)?;
        metrics.record_changed_files(count);
        save(synrepo_dir, &metrics)
    })() {
        tracing::debug!(%error, "context changed-file metrics record failed");
    }
}

fn metrics_path(synrepo_dir: &Path) -> std::path::PathBuf {
    synrepo_dir.join("state").join(METRICS_FILE)
}

fn budget_label(budget: Budget) -> &'static str {
    match budget {
        Budget::Tiny => "tiny",
        Budget::Normal => "normal",
        Budget::Deep => "deep",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_card_updates_token_and_budget_totals() {
        let accounting = ContextAccounting::new(Budget::Tiny, 100, 1_000, vec!["hash".to_string()]);
        let mut metrics = ContextMetrics::default();
        metrics.record_card(&accounting, 25);

        assert_eq!(metrics.cards_served_total, 1);
        assert_eq!(metrics.card_tokens_total, 100);
        assert_eq!(metrics.raw_file_tokens_total, 1_000);
        assert_eq!(metrics.estimated_tokens_saved_total, 900);
        assert_eq!(metrics.budget_tier_usage.get("tiny"), Some(&1));
        assert_eq!(metrics.context_query_latency_ms_avg(), 25.0);
    }
}
