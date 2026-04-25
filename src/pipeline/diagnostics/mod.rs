//! Operational diagnostics surface for `.synrepo/` runtime health.
//!
//! Exposes observable state about reconcile health, writer ownership, and
//! store compatibility, so stale or unhealthy runtime conditions are visible
//! rather than silent. The goal is to make background behavior inspectable
//! without building a large ops dashboard.

mod health;
mod types;

use std::path::Path;

use time::OffsetDateTime;

use crate::config::Config;

use super::watch::{load_reconcile_state, watch_service_status};
use health::{
    compute_embedding_health, compute_reconcile_health, compute_store_guidance,
    compute_writer_status,
};

pub use types::{
    EmbeddingHealth, ReconcileHealth, ReconcileStaleness, RuntimeDiagnostics, WriterStatus,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::structural::CompileSummary;
    use crate::pipeline::watch::{persist_reconcile_state, ReconcileOutcome};
    use crate::pipeline::writer::{acquire_writer_lock, writer_lock_path, WriterOwnership};
    #[cfg(unix)]
    use crate::pipeline::writer::{hold_writer_flock_with_ownership, live_foreign_pid};
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
    fn diagnostics_shows_held_by_self_when_lock_held() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
        let diag = collect_diagnostics(&synrepo_dir, &Config::default());

        assert_eq!(diag.writer_status, WriterStatus::HeldBySelf);
    }

    #[test]
    fn diagnostics_ignores_stale_writer_metadata_when_flock_is_free() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

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
        assert_eq!(diag.writer_status, WriterStatus::Free);
    }

    #[test]
    #[cfg(unix)]
    fn diagnostics_shows_held_by_other_when_foreign_flock_is_held() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let (mut child, pid) = live_foreign_pid();
        let _flock = hold_writer_flock_with_ownership(
            &writer_lock_path(&synrepo_dir),
            &WriterOwnership {
                pid,
                acquired_at: "2024-01-01T00:00:00Z".to_string(),
            },
        );

        let diag = collect_diagnostics(&synrepo_dir, &Config::default());
        assert_eq!(diag.writer_status, WriterStatus::HeldByOther { pid });

        let _ = child.kill();
        let _ = child.wait();
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
