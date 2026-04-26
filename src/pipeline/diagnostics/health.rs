//! Pure compute helpers for runtime diagnostics.
//!
//! Each helper maps a raw state (reconcile state, writer-lock ownership,
//! storage compatibility, embedding index) to one of the typed health enums
//! exposed from `types.rs`. They never mutate state and take only the inputs
//! they need so they can be unit-tested without filesystem fixtures.

use std::path::Path;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use super::types::{EmbeddingHealth, ReconcileHealth, ReconcileStaleness, WriterStatus};
use crate::config::Config;
use crate::pipeline::watch::{ReconcileState, ReconcileStateError};
use crate::pipeline::writer::{
    current_ownership, open_and_try_lock, writer_lock_path, WriterOwnership, WriterOwnershipError,
};

/// Maximum time since the last reconcile (in seconds) before it is considered stale.
const RECONCILE_STALENESS_THRESHOLD_SECONDS: i64 = 3600;

pub(super) fn compute_reconcile_health(
    state_result: &Result<ReconcileState, ReconcileStateError>,
    now: OffsetDateTime,
    watch_running: bool,
) -> ReconcileHealth {
    match state_result {
        Err(ReconcileStateError::NotFound) => ReconcileHealth::Unknown,
        Err(ReconcileStateError::Malformed(e)) => ReconcileHealth::Corrupt(e.clone()),
        Ok(s) if s.last_outcome == "completed" => {
            if watch_running {
                // If the watch service is running, it is responsible for
                // observing changes. We trust it to keep the graph current
                // and skip the age-based "stale" nudge.
                return ReconcileHealth::Current;
            }

            let last_ts = OffsetDateTime::parse(&s.last_reconcile_at, &Rfc3339).ok();
            let is_old = last_ts
                .map(|ts| (now - ts).whole_seconds().abs() >= RECONCILE_STALENESS_THRESHOLD_SECONDS)
                .unwrap_or(false);

            if is_old {
                ReconcileHealth::Stale(ReconcileStaleness::Age {
                    last_reconcile_at: s.last_reconcile_at.clone(),
                })
            } else {
                ReconcileHealth::Current
            }
        }
        Ok(s) => ReconcileHealth::Stale(ReconcileStaleness::Outcome(s.last_outcome.clone())),
    }
}

pub(super) fn compute_writer_status(synrepo_dir: &Path) -> WriterStatus {
    match current_ownership(synrepo_dir) {
        Err(WriterOwnershipError::NotFound) => WriterStatus::Free,
        Err(WriterOwnershipError::Malformed(e)) => WriterStatus::Corrupt(e),
        Ok(WriterOwnership { pid, .. }) => {
            match open_and_try_lock(&writer_lock_path(synrepo_dir)) {
                Ok(Some(_file)) => WriterStatus::Free,
                Ok(None) if pid == std::process::id() => WriterStatus::HeldBySelf,
                Ok(None) => WriterStatus::HeldByOther { pid },
                Err(err) => WriterStatus::Corrupt(err.to_string()),
            }
        }
    }
}

pub(super) fn compute_store_guidance(synrepo_dir: &Path, config: &Config) -> Vec<String> {
    let runtime_exists = synrepo_dir.exists();
    match crate::store::compatibility::evaluate_runtime(synrepo_dir, runtime_exists, config) {
        Ok(report) => report.guidance_lines(),
        Err(err) => vec![format!("could not evaluate storage compatibility: {err}")],
    }
}

#[cfg(feature = "semantic-triage")]
pub(super) fn compute_embedding_health(synrepo_dir: &Path, config: &Config) -> EmbeddingHealth {
    if !config.enable_semantic_triage {
        return EmbeddingHealth::Disabled;
    }

    let index_path = synrepo_dir.join("index/vectors/index.bin");
    if !index_path.exists() {
        return EmbeddingHealth::Degraded(
            "embedding index missing; run `synrepo reconcile` to build it".to_string(),
        );
    }

    match crate::substrate::embedding::index::FlatVecIndex::load(&index_path, config.embedding_dim)
    {
        Ok(index) => {
            let model_cached = crate::substrate::embedding::model::get_global_cache_dir()
                .ok()
                .map(|d| d.join(config.semantic_model.replace('/', "--")))
                .is_some_and(|d| d.join("model.onnx").exists());

            if !model_cached {
                return EmbeddingHealth::Degraded(format!(
                    "model '{}' not cached locally; will be downloaded on next use",
                    config.semantic_model
                ));
            }

            EmbeddingHealth::Available {
                model: config.semantic_model.clone(),
                dim: config.embedding_dim,
                chunks: index.len(),
            }
        }
        Err(e) => EmbeddingHealth::Degraded(format!("index load failed: {e}")),
    }
}

#[cfg(not(feature = "semantic-triage"))]
pub(super) fn compute_embedding_health(_synrepo_dir: &Path, config: &Config) -> EmbeddingHealth {
    if config.enable_semantic_triage {
        // Config says enabled but the feature is not compiled in.
        EmbeddingHealth::Degraded(
            "semantic triage enabled in config but not compiled in (rebuild with --features semantic-triage)".to_string(),
        )
    } else {
        EmbeddingHealth::Disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_reconcile_health_shows_stale_when_completed_but_old() {
        let state = ReconcileState {
            last_reconcile_at: "2024-01-01T12:00:00Z".to_string(),
            last_outcome: "completed".to_string(),
            last_error: None,
            triggering_events: 0,
            files_discovered: Some(10),
            symbols_extracted: Some(50),
        };

        // 2 hours later
        let now = OffsetDateTime::parse("2024-01-01T14:00:00Z", &Rfc3339).unwrap();
        let health = compute_reconcile_health(&Ok(state), now, false);

        assert!(
            matches!(
                health,
                ReconcileHealth::Stale(ReconcileStaleness::Age { .. })
            ),
            "expected Stale(Age), got {:?}",
            health
        );
    }

    #[test]
    fn compute_reconcile_health_skips_age_check_when_watch_is_running() {
        let state = ReconcileState {
            last_reconcile_at: "2024-01-01T12:00:00Z".to_string(),
            last_outcome: "completed".to_string(),
            last_error: None,
            triggering_events: 0,
            files_discovered: Some(10),
            symbols_extracted: Some(50),
        };

        // 2 hours later, but watch is running
        let now = OffsetDateTime::parse("2024-01-01T14:00:00Z", &Rfc3339).unwrap();
        let health = compute_reconcile_health(&Ok(state), now, true);

        assert_eq!(health, ReconcileHealth::Current);
    }

    #[test]
    fn compute_reconcile_health_shows_current_when_completed_and_recent() {
        let state = ReconcileState {
            last_reconcile_at: "2024-01-01T12:00:00Z".to_string(),
            last_outcome: "completed".to_string(),
            last_error: None,
            triggering_events: 0,
            files_discovered: Some(10),
            symbols_extracted: Some(50),
        };

        // 30 minutes later
        let now = OffsetDateTime::parse("2024-01-01T12:30:00Z", &Rfc3339).unwrap();
        let health = compute_reconcile_health(&Ok(state), now, false);

        assert_eq!(health, ReconcileHealth::Current);
    }
}
