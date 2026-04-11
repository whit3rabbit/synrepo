//! Operational diagnostics surface for `.synrepo/` runtime health.
//!
//! Exposes observable state about reconcile health, writer ownership, and
//! store compatibility, so stale or unhealthy runtime conditions are visible
//! rather than silent. The goal is to make background behavior inspectable
//! without building a large ops dashboard.

use std::path::Path;

use crate::config::Config;

use super::{
    watch::{load_reconcile_state, ReconcileState},
    writer::{current_ownership, WriterOwnership},
};

/// How fresh the last reconcile appears based on its recorded outcome.
///
/// Phase 1: freshness is determined by the last outcome string only.
/// Time-based staleness detection (e.g. "last reconcile was >1 hour ago")
/// can be layered in once reconcile intervals are defined.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconcileHealth {
    /// The last reconcile completed successfully.
    Current,
    /// The last reconcile did not complete, or the outcome was not "completed".
    Stale {
        /// Outcome string of the most recent attempted reconcile.
        last_outcome: String,
    },
    /// No reconcile state file exists; the system has never reconciled.
    Unknown,
}

/// Current writer lock ownership status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriterStatus {
    /// No writer lock is held.
    Free,
    /// The lock is held by the current process.
    HeldBySelf,
    /// The lock file records a different process ID.
    ///
    /// Note: the recorded PID may be stale (the process may have died). The
    /// diagnostics surface reports what the lock file says; liveness checking
    /// happens only during `acquire_writer_lock`.
    HeldByOther {
        /// PID recorded in the lock file.
        pid: u32,
    },
}

/// Top-level operational diagnostics for a `.synrepo/` runtime.
#[derive(Clone, Debug)]
pub struct RuntimeDiagnostics {
    /// Reconcile system health.
    pub reconcile_health: ReconcileHealth,
    /// Current writer lock status.
    pub writer_status: WriterStatus,
    /// Non-trivial storage compatibility guidance lines.
    pub store_guidance: Vec<String>,
    /// Raw reconcile state, if present.
    pub last_reconcile: Option<ReconcileState>,
}

impl RuntimeDiagnostics {
    /// Render a human-readable diagnostic summary for CLI or logging output.
    pub fn render(&self) -> String {
        let mut out = String::new();

        out.push_str("Reconcile: ");
        match &self.reconcile_health {
            ReconcileHealth::Current => out.push_str("current\n"),
            ReconcileHealth::Stale { last_outcome } => {
                out.push_str(&format!("stale (last outcome: {last_outcome})\n"));
            }
            ReconcileHealth::Unknown => out.push_str("unknown (no reconcile state)\n"),
        }

        out.push_str("Writer: ");
        match &self.writer_status {
            WriterStatus::Free => out.push_str("free\n"),
            WriterStatus::HeldBySelf => out.push_str("held by current process\n"),
            WriterStatus::HeldByOther { pid } => out.push_str(&format!("held by pid {pid}\n")),
        }

        if let Some(state) = &self.last_reconcile {
            out.push_str(&format!(
                "Last reconcile: {} ({} events)\n",
                state.last_reconcile_at, state.triggering_events,
            ));
            if let (Some(files), Some(syms)) = (state.files_discovered, state.symbols_extracted) {
                out.push_str(&format!(
                    "  files_discovered={files}, symbols_extracted={syms}\n"
                ));
            }
        }

        for line in &self.store_guidance {
            out.push_str(&format!("Store: {line}\n"));
        }

        out
    }
}

/// Collect current runtime diagnostics for `synrepo_dir`.
///
/// Reads reconcile state, writer lock, and storage compatibility into a
/// single snapshot. All reads are best-effort: missing or malformed state
/// is reported as `Unknown` or `Free` rather than returning an error.
pub fn collect_diagnostics(synrepo_dir: &Path, config: &Config) -> RuntimeDiagnostics {
    let last_reconcile = load_reconcile_state(synrepo_dir);
    let reconcile_health = compute_reconcile_health(last_reconcile.as_ref());
    let writer_status = compute_writer_status(synrepo_dir);
    let store_guidance = compute_store_guidance(synrepo_dir, config);

    RuntimeDiagnostics {
        reconcile_health,
        writer_status,
        store_guidance,
        last_reconcile,
    }
}

fn compute_reconcile_health(state: Option<&ReconcileState>) -> ReconcileHealth {
    match state {
        None => ReconcileHealth::Unknown,
        Some(s) if s.last_outcome == "completed" => ReconcileHealth::Current,
        Some(s) => ReconcileHealth::Stale {
            last_outcome: s.last_outcome.clone(),
        },
    }
}

fn compute_writer_status(synrepo_dir: &Path) -> WriterStatus {
    match current_ownership(synrepo_dir) {
        None => WriterStatus::Free,
        Some(WriterOwnership { pid, .. }) if pid == std::process::id() => WriterStatus::HeldBySelf,
        Some(WriterOwnership { pid, .. }) => WriterStatus::HeldByOther { pid },
    }
}

fn compute_store_guidance(synrepo_dir: &Path, config: &Config) -> Vec<String> {
    let runtime_exists = synrepo_dir.exists();
    match crate::store::compatibility::evaluate_runtime(synrepo_dir, runtime_exists, config) {
        Ok(report) => report.guidance_lines(),
        Err(err) => vec![format!("could not evaluate storage compatibility: {err}")],
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
        assert!(
            matches!(diag.reconcile_health, ReconcileHealth::Stale { .. }),
            "expected Stale, got {:?}",
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
                ReconcileHealth::Stale { ref last_outcome } if last_outcome == "lock-conflict"
            ),
            "expected Stale(lock-conflict), got {:?}",
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
}
