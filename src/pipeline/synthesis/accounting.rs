//! Synthesis accounting: append-only per-call log plus an aggregates
//! snapshot, both under `.synrepo/state/`.
//!
//! Two files:
//!
//! - `.synrepo/state/synthesis-log.jsonl` — one JSON record per call, written
//!   via plain append. Crash-safe for our record sizes (small JSON lines
//!   well under a filesystem page).
//! - `.synrepo/state/synthesis-totals.json` — small aggregates blob.
//!   Rewritten on each update via [`crate::util::atomic_write::atomic_write`]
//!   (temp file + fsync + rename) so a crash never leaves it truncated.
//!
//! This module does not hold any long-lived state; [`record_event`] is
//! invoked synchronously from [`super::telemetry::publish`] after every
//! event is fanned out. Each call opens the files it needs, writes, and
//! closes them. That keeps the surface compatible with short-lived CLI
//! processes (`synrepo sync`, `synrepo reconcile`) without requiring a
//! writer thread.
//!
//! USD cost lookups consult [`super::pricing`]; unknown `(provider, model)`
//! pairs record `usd_cost: null` rather than guessing, and
//! `any_estimated` on the totals snapshot flips to `true` the first time
//! any estimated-usage call is seen.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::pricing;
use super::telemetry::{Outcome, SynthesisEvent, SynthesisTarget, UsageSource};
use crate::util::atomic_write::atomic_write;

const LOG_FILENAME: &str = "synthesis-log.jsonl";
const TOTALS_FILENAME: &str = "synthesis-totals.json";

/// Path of the synthesis append-only log for a given `.synrepo/` dir.
pub fn log_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(LOG_FILENAME)
}

/// Path of the synthesis totals snapshot for a given `.synrepo/` dir.
pub fn totals_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(TOTALS_FILENAME)
}

/// Per-provider rollup inside the totals snapshot.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderTotals {
    /// Number of completed calls for this provider.
    pub calls: u64,
    /// Total input tokens across all calls (may mix reported + estimated).
    pub input_tokens: u64,
    /// Total output tokens across all calls (may mix reported + estimated).
    pub output_tokens: u64,
    /// Computed USD cost. `None` when at least one call used an unknown
    /// `(provider, model)` pair; the user sees this as "cost unknown".
    #[serde(default)]
    pub usd_cost: Option<f64>,
}

/// Aggregates snapshot. Rewritten atomically on each call.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SynthesisTotals {
    /// RFC-3339 timestamp of the first recorded call (or the most recent
    /// reset, if `synrepo sync --reset-synthesis-totals` was used).
    #[serde(default)]
    pub since: Option<String>,
    /// RFC-3339 timestamp of the most recent event.
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Total successful calls.
    #[serde(default)]
    pub calls: u64,
    /// Total input tokens across all successful calls.
    #[serde(default)]
    pub input_tokens: u64,
    /// Total output tokens across all successful calls.
    #[serde(default)]
    pub output_tokens: u64,
    /// Total failed calls (HTTP / parse / transport errors).
    #[serde(default)]
    pub failures: u64,
    /// Total budget-blocked calls (refused before the network).
    #[serde(default)]
    pub budget_blocked: u64,
    /// Sum of usd_cost across calls with a known `(provider, model)`.
    #[serde(default)]
    pub usd_cost: f64,
    /// `true` once at least one call's tokens came from an estimate rather
    /// than a provider-reported count.
    #[serde(default)]
    pub any_estimated: bool,
    /// `true` once at least one call had an unknown `(provider, model)`
    /// and could not be priced.
    #[serde(default)]
    pub any_unpriced: bool,
    /// Per-provider rollups.
    #[serde(default)]
    pub per_provider: HashMap<String, ProviderTotals>,
}

/// One JSONL record in `synthesis-log.jsonl`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SynthesisCallRecord {
    /// RFC-3339 timestamp the record was emitted.
    pub timestamp: String,
    /// Correlation id.
    pub call_id: u64,
    /// Provider label (stable, lowercase).
    pub provider: String,
    /// Model identifier used for the call.
    pub model: String,
    /// Kind of target: "commentary" | "cross_link".
    pub target_kind: String,
    /// Node id in display form (commentary path: one id; cross-link path:
    /// `"from → to"`).
    pub target_label: String,
    /// Outcome label.
    pub outcome: String,
    /// Wall-clock duration when applicable (zero on budget-blocked).
    #[serde(default)]
    pub duration_ms: u64,
    /// Input / prompt tokens. `0` on budget-blocked.
    #[serde(default)]
    pub input_tokens: u32,
    /// Output / completion tokens. `0` on budget-blocked or failure.
    #[serde(default)]
    pub output_tokens: u32,
    /// Source of the counts ("reported" | "estimated"). Empty on budget
    /// blocks or failures.
    #[serde(default)]
    pub usage_source: String,
    /// USD cost or `null` if the model is not in the rate table.
    #[serde(default)]
    pub usd_cost: Option<f64>,
    /// Short truncated error on failure. Empty otherwise.
    #[serde(default)]
    pub error_tail: String,
}

/// Synchronously record a synthesis event: append a JSONL line for
/// lifecycle-terminal events, then rewrite the totals snapshot.
///
/// `CallStarted` events are skipped — the log records only terminal
/// outcomes (`CallCompleted`, `CallFailed`, `BudgetBlocked`). This keeps
/// one line per call and avoids double-counting.
pub fn record_event(synrepo_dir: &Path, event: &SynthesisEvent) -> std::io::Result<()> {
    let Some(record) = record_for_event(event) else {
        return Ok(());
    };
    append_record(synrepo_dir, &record)?;
    update_totals(synrepo_dir, &record)?;
    Ok(())
}

/// Reset the JSONL log and totals snapshot. The existing log is rotated to
/// `synthesis-log.jsonl.<rfc3339>.bak` so nothing is lost; the totals file
/// is replaced with a fresh zeroed snapshot.
pub fn reset(synrepo_dir: &Path) -> std::io::Result<()> {
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir)?;

    let log = log_path(synrepo_dir);
    if log.exists() {
        let suffix = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "unknown".to_string())
            .replace(':', "-");
        let backup = state_dir.join(format!("{LOG_FILENAME}.{suffix}.bak"));
        std::fs::rename(&log, &backup)?;
    }

    let totals = SynthesisTotals {
        since: Some(
            OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default(),
        ),
        ..SynthesisTotals::default()
    };
    let body = serde_json::to_vec_pretty(&totals)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    atomic_write(&totals_path(synrepo_dir), &body)?;
    Ok(())
}

/// Read the current totals snapshot, if any. Used by the Health tab and
/// `synrepo status --json`. `Ok(None)` means "no snapshot yet" (fresh
/// repo, never ran synthesis).
pub fn load_totals(synrepo_dir: &Path) -> std::io::Result<Option<SynthesisTotals>> {
    let path = totals_path(synrepo_dir);
    match std::fs::read(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
        Ok(body) => match serde_json::from_slice::<SynthesisTotals>(&body) {
            Ok(t) => Ok(Some(t)),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        },
    }
}

fn record_for_event(event: &SynthesisEvent) -> Option<SynthesisCallRecord> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();
    match event {
        SynthesisEvent::CallStarted { .. } => None,
        SynthesisEvent::CallCompleted {
            call_id,
            provider,
            model,
            target,
            duration_ms,
            usage,
            output_bytes: _,
        } => {
            let cost =
                pricing::cost_for_call(provider, model, usage.input_tokens, usage.output_tokens);
            Some(SynthesisCallRecord {
                timestamp: now,
                call_id: *call_id,
                provider: (*provider).to_string(),
                model: model.clone(),
                target_kind: target_kind_label(target).to_string(),
                target_label: target.display_label(),
                outcome: Outcome::Success.as_str().to_string(),
                duration_ms: *duration_ms,
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                usage_source: usage.source.as_str().to_string(),
                usd_cost: cost,
                error_tail: String::new(),
            })
        }
        SynthesisEvent::BudgetBlocked {
            call_id,
            provider,
            model,
            target,
            estimated_tokens,
            budget: _,
        } => Some(SynthesisCallRecord {
            timestamp: now,
            call_id: *call_id,
            provider: (*provider).to_string(),
            model: model.clone(),
            target_kind: target_kind_label(target).to_string(),
            target_label: target.display_label(),
            outcome: Outcome::BudgetBlocked.as_str().to_string(),
            duration_ms: 0,
            input_tokens: *estimated_tokens,
            output_tokens: 0,
            usage_source: UsageSource::Estimated.as_str().to_string(),
            usd_cost: None,
            error_tail: String::new(),
        }),
        SynthesisEvent::CallFailed {
            call_id,
            provider,
            model,
            target,
            duration_ms,
            error,
        } => Some(SynthesisCallRecord {
            timestamp: now,
            call_id: *call_id,
            provider: (*provider).to_string(),
            model: model.clone(),
            target_kind: target_kind_label(target).to_string(),
            target_label: target.display_label(),
            outcome: Outcome::Failed.as_str().to_string(),
            duration_ms: *duration_ms,
            input_tokens: 0,
            output_tokens: 0,
            usage_source: String::new(),
            usd_cost: None,
            error_tail: error.clone(),
        }),
    }
}

fn target_kind_label(target: &SynthesisTarget) -> &'static str {
    match target {
        SynthesisTarget::Commentary { .. } => "commentary",
        SynthesisTarget::CrossLink { .. } => "cross_link",
    }
}

fn append_record(synrepo_dir: &Path, record: &SynthesisCallRecord) -> std::io::Result<()> {
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir)?;
    let path = log_path(synrepo_dir);
    let mut f: File = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writeln!(f, "{line}")?;
    Ok(())
}

fn update_totals(synrepo_dir: &Path, record: &SynthesisCallRecord) -> std::io::Result<()> {
    let mut totals = load_totals(synrepo_dir)?.unwrap_or_default();
    if totals.since.is_none() {
        totals.since = Some(record.timestamp.clone());
    }
    totals.updated_at = Some(record.timestamp.clone());

    match record.outcome.as_str() {
        s if s == Outcome::Success.as_str() => {
            totals.calls = totals.calls.saturating_add(1);
            totals.input_tokens = totals
                .input_tokens
                .saturating_add(record.input_tokens as u64);
            totals.output_tokens = totals
                .output_tokens
                .saturating_add(record.output_tokens as u64);
            if record.usage_source == UsageSource::Estimated.as_str() {
                totals.any_estimated = true;
            }
            match record.usd_cost {
                Some(c) => totals.usd_cost += c,
                None => totals.any_unpriced = true,
            }

            let entry = totals
                .per_provider
                .entry(record.provider.clone())
                .or_default();
            entry.calls = entry.calls.saturating_add(1);
            entry.input_tokens = entry
                .input_tokens
                .saturating_add(record.input_tokens as u64);
            entry.output_tokens = entry
                .output_tokens
                .saturating_add(record.output_tokens as u64);
            match record.usd_cost {
                Some(c) => {
                    entry.usd_cost = Some(entry.usd_cost.unwrap_or(0.0) + c);
                }
                None => entry.usd_cost = None,
            }
        }
        s if s == Outcome::Failed.as_str() => {
            totals.failures = totals.failures.saturating_add(1);
        }
        s if s == Outcome::BudgetBlocked.as_str() => {
            totals.budget_blocked = totals.budget_blocked.saturating_add(1);
        }
        _ => {}
    }

    let body = serde_json::to_vec_pretty(&totals)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    atomic_write(&totals_path(synrepo_dir), &body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::telemetry::{SynthesisEvent, SynthesisTarget, TokenUsage};
    use super::*;
    use crate::core::ids::{FileNodeId, NodeId};
    use tempfile::tempdir;

    fn file_target(n: u128) -> SynthesisTarget {
        SynthesisTarget::Commentary {
            node: NodeId::File(FileNodeId(n)),
        }
    }

    fn completed(call_id: u64, usage: TokenUsage) -> SynthesisEvent {
        SynthesisEvent::CallCompleted {
            call_id,
            provider: "openai",
            model: "gpt-4o-mini".to_string(),
            target: file_target(call_id as u128),
            duration_ms: 100,
            usage,
            output_bytes: 42,
        }
    }

    #[test]
    fn completed_event_appends_and_updates_totals() {
        let dir = tempdir().unwrap();
        let synrepo = dir.path();

        record_event(
            synrepo,
            &completed(1, TokenUsage::reported(1_000_000, 1_000_000)),
        )
        .unwrap();

        let log = std::fs::read_to_string(log_path(synrepo)).unwrap();
        assert_eq!(log.lines().count(), 1);
        let rec: SynthesisCallRecord = serde_json::from_str(log.lines().next().unwrap()).unwrap();
        assert_eq!(rec.provider, "openai");
        assert_eq!(rec.outcome, "success");
        assert!(rec.usd_cost.is_some());

        let totals = load_totals(synrepo).unwrap().unwrap();
        assert_eq!(totals.calls, 1);
        assert_eq!(totals.input_tokens, 1_000_000);
        assert!(!totals.any_estimated);
        assert!(!totals.any_unpriced);
        let per = totals.per_provider.get("openai").unwrap();
        assert_eq!(per.calls, 1);
    }

    #[test]
    fn estimated_usage_flips_any_estimated_bit() {
        let dir = tempdir().unwrap();
        record_event(dir.path(), &completed(1, TokenUsage::estimated(100, 100))).unwrap();
        let totals = load_totals(dir.path()).unwrap().unwrap();
        assert!(totals.any_estimated);
    }

    #[test]
    fn unknown_model_flips_any_unpriced_bit_and_records_null_cost() {
        let dir = tempdir().unwrap();
        let event = SynthesisEvent::CallCompleted {
            call_id: 1,
            provider: "openai",
            model: "unknown-future-model".to_string(),
            target: file_target(1),
            duration_ms: 10,
            usage: TokenUsage::reported(100, 100),
            output_bytes: 5,
        };
        record_event(dir.path(), &event).unwrap();
        let totals = load_totals(dir.path()).unwrap().unwrap();
        assert!(totals.any_unpriced);
        let log = std::fs::read_to_string(log_path(dir.path())).unwrap();
        let rec: SynthesisCallRecord = serde_json::from_str(log.lines().next().unwrap()).unwrap();
        assert!(rec.usd_cost.is_none());
    }

    #[test]
    fn failed_event_records_error_and_bumps_failures() {
        let dir = tempdir().unwrap();
        let event = SynthesisEvent::CallFailed {
            call_id: 1,
            provider: "openai",
            model: "gpt-4o-mini".to_string(),
            target: file_target(1),
            duration_ms: 250,
            error: "HTTP 500: upstream down".to_string(),
        };
        record_event(dir.path(), &event).unwrap();
        let totals = load_totals(dir.path()).unwrap().unwrap();
        assert_eq!(totals.failures, 1);
        assert_eq!(totals.calls, 0);
        let log = std::fs::read_to_string(log_path(dir.path())).unwrap();
        let rec: SynthesisCallRecord = serde_json::from_str(log.lines().next().unwrap()).unwrap();
        assert!(rec.error_tail.contains("HTTP 500"));
    }

    #[test]
    fn reset_rotates_log_and_zeros_totals() {
        let dir = tempdir().unwrap();
        record_event(dir.path(), &completed(1, TokenUsage::reported(10, 10))).unwrap();
        reset(dir.path()).unwrap();

        assert!(!log_path(dir.path()).exists(), "log should be rotated");
        // A backup file with the .bak suffix should exist.
        let state = dir.path().join("state");
        let baks: Vec<_> = std::fs::read_dir(state)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
            .collect();
        assert_eq!(baks.len(), 1);

        let totals = load_totals(dir.path()).unwrap().unwrap();
        assert_eq!(totals.calls, 0);
        assert_eq!(totals.input_tokens, 0);
        assert!(totals.since.is_some());
    }

    #[test]
    fn call_started_events_are_not_logged() {
        let dir = tempdir().unwrap();
        let event = SynthesisEvent::CallStarted {
            call_id: 1,
            provider: "openai",
            model: "gpt-4o-mini".to_string(),
            target: file_target(1),
            started_at_ms: 0,
        };
        record_event(dir.path(), &event).unwrap();
        assert!(!log_path(dir.path()).exists());
        assert!(!totals_path(dir.path()).exists());
    }
}
