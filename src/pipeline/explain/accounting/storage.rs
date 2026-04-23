use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::super::telemetry::{ExplainEvent, Outcome, UsageSource};
use super::record::record_for_event;
use super::types::{ExplainCallRecord, ExplainTotals};
use crate::util::atomic_write::atomic_write;

const LOG_FILENAME: &str = "explain-log.jsonl";
const TOTALS_FILENAME: &str = "explain-totals.json";

/// Path of the explain append-only log for a given `.synrepo/` dir.
pub fn log_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(LOG_FILENAME)
}

/// Path of the explain totals snapshot for a given `.synrepo/` dir.
pub fn totals_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(TOTALS_FILENAME)
}

/// Synchronously record a explain event: append a JSONL line for
/// lifecycle-terminal events, then rewrite the totals snapshot.
///
/// `CallStarted` events are skipped.
pub fn record_event(synrepo_dir: &Path, event: &ExplainEvent) -> std::io::Result<()> {
    let Some(record) = record_for_event(event) else {
        return Ok(());
    };
    append_record(synrepo_dir, &record)?;
    update_totals(synrepo_dir, &record)?;
    Ok(())
}

/// Reset the JSONL log and totals snapshot. The existing log is rotated to
/// `explain-log.jsonl.<rfc3339>.bak` so nothing is lost.
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

    let totals = ExplainTotals {
        since: Some(
            OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default(),
        ),
        ..ExplainTotals::default()
    };
    atomic_write_json(&totals_path(synrepo_dir), &totals)
}

/// Read the current totals snapshot, if any.
pub fn load_totals(synrepo_dir: &Path) -> std::io::Result<Option<ExplainTotals>> {
    match std::fs::read(totals_path(synrepo_dir)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
        Ok(body) => serde_json::from_slice::<ExplainTotals>(&body)
            .map(Some)
            .map_err(invalid_data),
    }
}

fn append_record(synrepo_dir: &Path, record: &ExplainCallRecord) -> std::io::Result<()> {
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir)?;
    let mut file: File = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path(synrepo_dir))?;
    let line = serde_json::to_string(record).map_err(invalid_data)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn update_totals(synrepo_dir: &Path, record: &ExplainCallRecord) -> std::io::Result<()> {
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
                Some(cost) => totals.usd_cost += cost,
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
                Some(cost) => entry.usd_cost = Some(entry.usd_cost.unwrap_or(0.0) + cost),
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

    atomic_write_json(&totals_path(synrepo_dir), &totals)
}

fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let body = serde_json::to_vec_pretty(value).map_err(invalid_data)?;
    atomic_write(path, &body)
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error)
}
