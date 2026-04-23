//! Operational diagnostics surface for `.synrepo/` runtime health.
//!
//! Exposes observable state about reconcile health, writer ownership, and
//! store compatibility, so stale or unhealthy runtime conditions are visible
//! rather than silent. The goal is to make background behavior inspectable
//! without building a large ops dashboard.

mod types;

use std::path::Path;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::config::Config;

use super::{
    watch::{load_reconcile_state, watch_service_status, ReconcileState, ReconcileStateError},
    writer::{current_ownership, WriterOwnership, WriterOwnershipError},
};

pub use types::{
    EmbeddingHealth, ReconcileHealth, ReconcileStaleness, RuntimeDiagnostics, WriterStatus,
};

/// Maximum time since the last reconcile (in seconds) before it is considered stale.
const RECONCILE_STALENESS_THRESHOLD_SECONDS: i64 = 3600;

/// Collect current runtime diagnostics for `synrepo_dir`.
///
/// Reads reconcile state, writer lock, and storage compatibility into a
/// single snapshot. All reads are best-effort: missing or malformed state
/// is reported as `Unknown` or `Free` rather than returning an error.
pub fn collect_diagnostics(synrepo_dir: &Path, config: &Config) -> RuntimeDiagnostics {
    let last_reconcile = load_reconcile_state(synrepo_dir);
    let reconcile_health = compute_reconcile_health(&last_reconcile, OffsetDateTime::now_utc());
    let watch_status = watch_service_status(synrepo_dir);
    let writer_status = compute_writer_status(synrepo_dir);
    let store_guidance = compute_store_guidance(synrepo_dir, config);
    let embedding_health = compute_embedding_health(synrepo_dir, config);

    RuntimeDiagnostics {
        reconcile_health,
        watch_status,
        writer_status,
        store_guidance,
        last_reconcile: last_reconcile.ok(),
        embedding_health,
    }
}
fn compute_reconcile_health(
    state_result: &Result<ReconcileState, ReconcileStateError>,
    now: OffsetDateTime,
) -> ReconcileHealth {
    match state_result {
        Err(ReconcileStateError::NotFound) => ReconcileHealth::Unknown,
        Err(ReconcileStateError::Malformed(e)) => ReconcileHealth::Corrupt(e.clone()),
        Ok(s) if s.last_outcome == "completed" => {
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

fn compute_writer_status(synrepo_dir: &Path) -> WriterStatus {
    match current_ownership(synrepo_dir) {
        Err(WriterOwnershipError::NotFound) => WriterStatus::Free,
        Err(WriterOwnershipError::Malformed(e)) => WriterStatus::Corrupt(e),
        Ok(WriterOwnership { pid, .. }) if pid == std::process::id() => WriterStatus::HeldBySelf,
        Ok(WriterOwnership { pid, .. }) => WriterStatus::HeldByOther { pid },
    }
}

fn compute_store_guidance(synrepo_dir: &Path, config: &Config) -> Vec<String> {
    let runtime_exists = synrepo_dir.exists();
    match crate::store::compatibility::evaluate_runtime(synrepo_dir, runtime_exists, config) {
        Ok(report) => report.guidance_lines(),
        Err(err) => vec![format!("could not evaluate storage compatibility: {err}")],
    }
}

#[cfg(feature = "semantic-triage")]
fn compute_embedding_health(synrepo_dir: &Path, config: &Config) -> EmbeddingHealth {
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
fn compute_embedding_health(_synrepo_dir: &Path, config: &Config) -> EmbeddingHealth {
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
    use crate::pipeline::structural::CompileSummary;
    use crate::pipeline::watch::{persist_reconcile_state, ReconcileOutcome};
    use crate::pipeline::writer::{acquire_writer_lock, writer_lock_path, WriterOwnership};
    use tempfile::tempdir;

    #[test]
    fn diagnostics_with_no_runtime_shows_unknown_reconcile_and_free_writer() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());

        assert_eq!(diag.reconcile_health, ReconcileHealth::Unknown);
        assert_eq!(diag.writer_status, WriterStatus::Free);
        assert!(diag.last_reconcile.is_none());
    }

    #[test]
    fn diagnostics_shows_current_after_completed_reconcile() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let summary = CompileSummary {
            files_discovered: 3,
            symbols_extracted: 7,
            ..Default::default()
        };
        persist_reconcile_state(&synrepo_dir, &ReconcileOutcome::Completed(summary), 2);

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        assert_eq!(diag.reconcile_health, ReconcileHealth::Current);
        assert_eq!(diag.writer_status, WriterStatus::Free);

        let state = diag.last_reconcile.unwrap();
        assert_eq!(state.files_discovered, Some(3));
        assert_eq!(state.symbols_extracted, Some(7));
    }

    #[test]
    fn diagnostics_shows_stale_after_failed_reconcile() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        persist_reconcile_state(
            &synrepo_dir,
            &ReconcileOutcome::Failed("graph locked".to_string()),
            0,
        );

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        let expected_outcome = "failed".to_string();
        assert!(
            matches!(diag.reconcile_health, ReconcileHealth::Stale(ReconcileStaleness::Outcome(ref o)) if o == &expected_outcome),
            "expected Stale(Outcome(failed)), got {:?}",
            diag.reconcile_health,
        );
    }

    #[test]
    fn diagnostics_shows_stale_after_lock_conflict() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        persist_reconcile_state(
            &synrepo_dir,
            &ReconcileOutcome::LockConflict { holder_pid: 99 },
            1,
        );

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        assert!(
            matches!(
                diag.reconcile_health,
                ReconcileHealth::Stale(ReconcileStaleness::Outcome(ref last_outcome)) if last_outcome == "lock-conflict"
            ),
            "expected Stale(Outcome(lock-conflict)), got {:?}",
            diag.reconcile_health,
        );
    }

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
        let health = compute_reconcile_health(&Ok(state), now);

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
        let health = compute_reconcile_health(&Ok(state), now);

        assert_eq!(health, ReconcileHealth::Current);
    }

    #[test]
    fn diagnostics_shows_held_by_self_when_lock_held() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
        let diag = collect_diagnostics(&synrepo_dir, &Config::default());

        assert_eq!(diag.writer_status, WriterStatus::HeldBySelf);
    }

    #[test]
    fn diagnostics_shows_held_by_other_when_foreign_pid_in_lock_file() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

        // Write a lock file with a foreign PID (42 is unlikely to be ours).
        let ownership = WriterOwnership {
            pid: 42,
            acquired_at: "2024-01-01T00:00:00Z".to_string(),
        };
        std::fs::write(
            writer_lock_path(&synrepo_dir),
            serde_json::to_string_pretty(&ownership).unwrap(),
        )
        .unwrap();

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        // Diagnostics reports what the lock file says without checking liveness.
        assert!(
            matches!(diag.writer_status, WriterStatus::HeldByOther { pid: 42 })
                || matches!(diag.writer_status, WriterStatus::Free),
            "expected HeldByOther(42) or Free (if PID 42 is ours), got {:?}",
            diag.writer_status,
        );
    }

    #[test]
    fn diagnostics_render_contains_unknown_state() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        let rendered = diag.render();
        assert!(
            rendered.contains("unknown"),
            "render must surface unknown reconcile state"
        );
        assert!(
            rendered.contains("free"),
            "render must surface free writer status"
        );
    }

    #[test]
    fn diagnostics_render_includes_reconcile_counts() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let summary = CompileSummary {
            files_discovered: 10,
            symbols_extracted: 30,
            ..Default::default()
        };
        persist_reconcile_state(&synrepo_dir, &ReconcileOutcome::Completed(summary), 5);

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        let rendered = diag.render();
        assert!(rendered.contains("files_discovered=10"));
        assert!(rendered.contains("symbols_extracted=30"));
    }

    #[test]
    fn diagnostics_shows_corrupt_when_reconcile_state_is_malformed() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let state_dir = synrepo_dir.join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("reconcile-state.json"), b"not valid json").unwrap();

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        assert!(
            matches!(diag.reconcile_health, ReconcileHealth::Corrupt(_)),
            "expected Corrupt, got {:?}",
            diag.reconcile_health,
        );
        assert!(diag.render().contains("corrupt"));
    }

    #[test]
    fn diagnostics_shows_corrupt_when_writer_lock_is_malformed() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let state_dir = synrepo_dir.join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("writer.lock"), b"not valid json").unwrap();

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        assert!(
            matches!(diag.writer_status, WriterStatus::Corrupt(_)),
            "expected Corrupt, got {:?}",
            diag.writer_status,
        );
        assert!(diag.render().contains("corrupt"));
    }

    #[test]
    fn embedding_health_disabled_when_triage_off() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        assert_eq!(diag.embedding_health, EmbeddingHealth::Disabled);
    }

    #[cfg(feature = "semantic-triage")]
    #[test]
    fn embedding_health_degraded_when_enabled_but_no_index() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let config = Config {
            enable_semantic_triage: true,
            ..Config::default()
        };
        let diag = collect_diagnostics(&synrepo_dir, &config);
        assert!(
            matches!(diag.embedding_health, EmbeddingHealth::Degraded(_)),
            "expected Degraded, got {:?}",
            diag.embedding_health
        );
    }
}
