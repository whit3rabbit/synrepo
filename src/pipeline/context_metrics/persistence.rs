//! Persistence and process-local batching for context metrics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::surface::card::ContextAccounting;
use crate::surface::task_route::TaskRoute;

use super::ContextMetrics;

const METRICS_FILE: &str = "context-metrics.json";
const FLUSH_AFTER_UPDATES: u64 = 16;
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct PendingMetrics {
    delta: ContextMetrics,
    updates_since_flush: u64,
    last_flush_attempt: Instant,
}

impl Default for PendingMetrics {
    fn default() -> Self {
        Self {
            delta: ContextMetrics::default(),
            updates_since_flush: 0,
            last_flush_attempt: Instant::now(),
        }
    }
}

static PENDING: OnceLock<Mutex<BTreeMap<PathBuf, PendingMetrics>>> = OnceLock::new();

/// Load context metrics. Missing files return empty metrics.
pub fn load(synrepo_dir: &Path) -> anyhow::Result<ContextMetrics> {
    let mut metrics = read_from_disk(synrepo_dir)?.unwrap_or_default();
    if let Some(delta) = pending_delta(synrepo_dir) {
        metrics.merge_from(&delta);
    }
    Ok(metrics)
}

/// Load context metrics only when the metrics file exists.
pub fn load_optional(synrepo_dir: &Path) -> anyhow::Result<Option<ContextMetrics>> {
    let disk = read_from_disk(synrepo_dir)?;
    let pending = pending_delta(synrepo_dir);
    match (disk, pending) {
        (None, None) => Ok(None),
        (Some(mut metrics), Some(delta)) => {
            metrics.merge_from(&delta);
            Ok(Some(metrics))
        }
        (Some(metrics), None) => Ok(Some(metrics)),
        (None, Some(delta)) => Ok(Some(delta)),
    }
}

/// Save context metrics.
pub fn save(synrepo_dir: &Path, metrics: &ContextMetrics) -> anyhow::Result<()> {
    write_to_disk(synrepo_dir, metrics)?;
    discard_pending(synrepo_dir);
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

/// Batched variant that records a whole response in memory before flushing
/// periodically. The same latency is attributed to every card in the batch.
pub fn record_cards_best_effort(
    synrepo_dir: &Path,
    accountings: &[ContextAccounting],
    latency_ms: u64,
    test_surface_hit: bool,
) {
    if accountings.is_empty() {
        return;
    }
    record_delta_best_effort(synrepo_dir, |metrics| {
        for accounting in accountings {
            metrics.record_card(accounting, latency_ms);
        }
        if test_surface_hit {
            metrics.record_test_surface_hit();
        }
    });
}

/// Best-effort changed-file metric recording.
pub fn record_changed_files_best_effort(synrepo_dir: &Path, count: usize) {
    if count == 0 {
        return;
    }
    record_delta_best_effort(synrepo_dir, |metrics| metrics.record_changed_files(count));
}

/// Best-effort recording of a workflow alias call (e.g. `"orient"`,
/// `"find"`, `"minimum_context"`). Canonical tool names are lowercase and
/// use underscore-separated form so they remain stable across client
/// surfaces. Failures are debug-only.
pub fn record_workflow_call_best_effort(synrepo_dir: &Path, tool: &str) {
    record_delta_best_effort(synrepo_dir, |metrics| metrics.record_workflow_call(tool));
}

/// Best-effort recording of a repository-scoped MCP tool request.
pub fn record_mcp_tool_result_best_effort(
    synrepo_dir: &Path,
    tool: &str,
    error_code: Option<&str>,
    saved_context_write: Option<&str>,
) {
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_mcp_tool_result(tool, error_code, saved_context_write);
    });
}

/// Best-effort recording of a repository-scoped MCP resource read.
pub fn record_mcp_resource_read_best_effort(synrepo_dir: &Path) {
    record_delta_best_effort(synrepo_dir, ContextMetrics::record_mcp_resource_read);
}

/// Best-effort recording of a compact MCP read output.
pub fn record_compact_output_best_effort(
    synrepo_dir: &Path,
    returned_token_estimate: usize,
    original_token_estimate: usize,
    estimated_tokens_saved: usize,
    omitted_count: usize,
    truncation_applied: bool,
) {
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_compact_output(
            returned_token_estimate,
            original_token_estimate,
            estimated_tokens_saved,
            omitted_count,
            truncation_applied,
        );
    });
}

/// Best-effort recording of final MCP response-budget behavior.
pub fn record_mcp_response_budget_best_effort(
    synrepo_dir: &Path,
    tool: &str,
    token_estimate: usize,
    over_soft_cap: bool,
    truncated: bool,
) {
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_mcp_response_budget(tool, token_estimate, over_soft_cap, truncated);
    });
}

/// Best-effort recording of context-pack aggregate tokens.
pub fn record_context_pack_tokens_best_effort(synrepo_dir: &Path, token_estimate: usize) {
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_context_pack_tokens(token_estimate);
    });
}

/// Best-effort recording of a task-route classification.
pub fn record_task_route_classification_best_effort(synrepo_dir: &Path, route: &TaskRoute) {
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_task_route_classification(route);
    });
}

/// Best-effort recording of route signals emitted from a nudge hook.
pub fn record_hook_route_emission_best_effort(synrepo_dir: &Path, route: &TaskRoute) {
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_hook_route_emission(route);
    });
}

/// Best-effort recording of accepted and rejected anchored edits.
pub fn record_anchored_edit_outcomes_best_effort(synrepo_dir: &Path, accepted: u64, rejected: u64) {
    if accepted == 0 && rejected == 0 {
        return;
    }
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_anchored_edit_outcomes(accepted, rejected);
    });
}

/// Best-effort recording of cross-link generation attempts.
pub fn record_cross_link_generation_best_effort(synrepo_dir: &Path, attempts: u64) {
    if attempts == 0 {
        return;
    }
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_cross_link_generation(attempts);
    });
}

/// Best-effort recording of promoted cross-links.
pub fn record_cross_link_promoted_best_effort(synrepo_dir: &Path, count: u64) {
    if count == 0 {
        return;
    }
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_cross_link_promoted(count);
    });
}

/// Best-effort recording of commentary refresh attempts.
pub fn record_commentary_refresh_best_effort(synrepo_dir: &Path, errored: bool) {
    record_delta_best_effort(synrepo_dir, |metrics| {
        metrics.record_commentary_refresh(errored);
    });
}

fn record_delta_best_effort<F>(synrepo_dir: &Path, record: F)
where
    F: FnOnce(&mut ContextMetrics),
{
    if let Err(error) = record_delta(synrepo_dir, record) {
        tracing::debug!(%error, "context metrics record failed");
    }
}

fn record_delta<F>(synrepo_dir: &Path, record: F) -> anyhow::Result<()>
where
    F: FnOnce(&mut ContextMetrics),
{
    let mut pending = pending_store()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    // Avoid the unconditional `to_path_buf()` allocation that `entry()` forces
    // when the slot already exists (the common case after the first call).
    if !pending.contains_key(synrepo_dir) {
        pending.insert(synrepo_dir.to_path_buf(), PendingMetrics::default());
    }
    let slot = pending.get_mut(synrepo_dir).expect("just inserted");
    record(&mut slot.delta);
    slot.updates_since_flush += 1;
    if should_flush(slot) {
        flush_slot(synrepo_dir, slot)?;
    }
    Ok(())
}

fn should_flush(slot: &PendingMetrics) -> bool {
    slot.updates_since_flush >= FLUSH_AFTER_UPDATES
        || slot.last_flush_attempt.elapsed() >= FLUSH_INTERVAL
}

fn flush_slot(synrepo_dir: &Path, slot: &mut PendingMetrics) -> anyhow::Result<()> {
    slot.last_flush_attempt = Instant::now();
    if slot.delta.is_empty() {
        slot.updates_since_flush = 0;
        return Ok(());
    }
    let mut metrics = read_from_disk(synrepo_dir)?.unwrap_or_default();
    metrics.merge_from(&slot.delta);
    write_to_disk(synrepo_dir, &metrics)?;
    slot.delta = ContextMetrics::default();
    slot.updates_since_flush = 0;
    Ok(())
}

fn pending_store() -> &'static Mutex<BTreeMap<PathBuf, PendingMetrics>> {
    PENDING.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn pending_delta(synrepo_dir: &Path) -> Option<ContextMetrics> {
    let pending = pending_store()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    pending
        .get(synrepo_dir)
        .map(|slot| slot.delta.clone())
        .filter(|delta| !delta.is_empty())
}

fn discard_pending(synrepo_dir: &Path) {
    let mut pending = pending_store()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    pending.remove(synrepo_dir);
}

fn read_from_disk(synrepo_dir: &Path) -> anyhow::Result<Option<ContextMetrics>> {
    let path = metrics_path(synrepo_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn write_to_disk(synrepo_dir: &Path, metrics: &ContextMetrics) -> anyhow::Result<()> {
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir)?;
    let bytes = serde_json::to_vec_pretty(metrics)?;
    crate::util::atomic_write::atomic_write(&metrics_path(synrepo_dir), &bytes)?;
    Ok(())
}

fn metrics_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(METRICS_FILE)
}

#[cfg(test)]
pub(super) fn flush_for_tests(synrepo_dir: &Path) -> anyhow::Result<()> {
    let mut pending = pending_store()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(slot) = pending.get_mut(synrepo_dir) {
        flush_slot(synrepo_dir, slot)?;
    }
    Ok(())
}
