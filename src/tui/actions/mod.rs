//! Operator-action dispatchers that bridge dashboard quick actions to the
//! existing control-plane primitives in `pipeline::watch::control`, the
//! decomposed `setup::step_*` helpers, and repair surfaces. The dashboard
//! never bypasses the writer lock: every mutating action flows through the
//! same admission path as the equivalent CLI subcommand, so watch-service
//! ownership and writer-lock ownership are both enforced.
//!
//! Actions return a structured [`ActionOutcome`] the dashboard can surface in
//! its bounded log pane. Callers lift the outcome into a [`LogEntry`] via
//! [`outcome_to_log`].

use std::path::{Path, PathBuf};

use crate::config::Config;

mod auto_sync;
mod helpers;
mod reconcile;
mod sync;
mod watch;

pub use auto_sync::set_auto_sync;
pub(crate) use helpers::now_rfc3339;
pub use helpers::{outcome_to_log, writer_lock_hint};
pub use reconcile::reconcile_now;
pub use sync::sync_now;
pub use watch::{start_watch_daemon, stop_watch};

/// Context passed to every action dispatcher. Keeps the callsite in the render
/// loop narrow; callers build this once per app state transition.
#[derive(Clone, Debug)]
pub struct ActionContext {
    /// Repo root (the `--repo` flag value or `cwd`).
    pub repo_root: PathBuf,
    /// Resolved `.synrepo/` directory.
    pub synrepo_dir: PathBuf,
}

impl ActionContext {
    /// Build a context from `repo_root`, deriving the `.synrepo/` path.
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            synrepo_dir: Config::synrepo_dir(repo_root),
        }
    }
}

/// Structured outcome of a dispatcher call. The dashboard never swallows these
/// — each one is surfaced in the log pane via [`outcome_to_log`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionOutcome {
    /// Action dispatched and the subsystem acknowledged it (e.g. watch stop).
    Ack {
        /// Short human-readable acknowledgement.
        message: String,
    },
    /// Action completed with an observable result (e.g. reconcile pass).
    Completed {
        /// Short human-readable summary.
        message: String,
    },
    /// Writer-lock or watch-ownership conflict. Structured so the log pane can
    /// name the holder PID and acquisition timestamp without the dashboard
    /// panicking or bypassing the lock.
    Conflict {
        /// PID of the foreign holder, when known.
        owner_pid: Option<u32>,
        /// RFC 3339 timestamp of the lock or lease acquisition, when known.
        acquired_at: Option<String>,
        /// Short description of the conflict surface (`"writer lock"`,
        /// `"watch lease"`, or the like).
        surface: String,
        /// Operator-facing guidance, one line.
        guidance: String,
    },
    /// Action failed with an error not attributable to a lock conflict.
    Error {
        /// Human-readable failure message.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn init_repo(path: &Path) {
        crate::bootstrap::bootstrap(path, None, false).expect("bootstrap");
    }

    fn isolated_home() -> (tempfile::TempDir, crate::config::test_home::HomeEnvGuard) {
        let home = tempfile::tempdir().unwrap();
        let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        (home, guard)
    }

    #[test]
    fn reconcile_now_on_fresh_repo_returns_error() {
        let dir = tempdir().unwrap();
        let ctx = ActionContext::new(dir.path());
        let outcome = reconcile_now(&ctx);
        // No config.toml → load fails with error (not a conflict).
        assert!(
            matches!(outcome, ActionOutcome::Error { .. }),
            "got {outcome:?}"
        );
    }

    #[test]
    fn reconcile_now_on_ready_repo_completes() {
        let _guard = crate::test_support::global_test_lock("tui-reconcile-now");
        let (_home, _home_guard) = isolated_home();
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());
        let outcome = reconcile_now(&ctx);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "expected Completed, got {outcome:?}"
        );
    }

    #[test]
    fn stop_watch_on_idle_repo_is_completed_noop() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());
        let outcome = stop_watch(&ctx);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
    }

    #[test]
    fn outcome_to_log_conflict_names_owner_pid() {
        let outcome = ActionOutcome::Conflict {
            owner_pid: Some(4242),
            acquired_at: Some("2026-04-17T12:00:00Z".to_string()),
            surface: "writer lock".to_string(),
            guidance: "held".to_string(),
        };
        let entry = outcome_to_log("lock", &outcome);
        assert_eq!(entry.severity, crate::tui::probe::Severity::Stale);
        assert!(entry.message.contains("owner pid 4242"));
        assert!(entry.message.contains("acquired 2026-04-17"));
    }

    // Phase 9.3: lock-contended reconcile-now must not panic and must surface
    // the conflict as a structured outcome. Linux/macOS only — the writer
    // flock helpers are unix-only.
    #[cfg(unix)]
    #[test]
    fn reconcile_now_under_foreign_lock_returns_conflict() {
        use crate::pipeline::writer::{
            hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
        };
        use std::fs;

        let _guard = crate::test_support::global_test_lock("tui-reconcile-now");
        let (_home, _home_guard) = isolated_home();
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());
        let lock_path = writer_lock_path(&ctx.synrepo_dir);
        fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let _guard = hold_writer_flock_with_ownership(
            &lock_path,
            &WriterOwnership {
                pid: 99999,
                acquired_at: "2026-04-17T12:00:00Z".to_string(),
            },
        );

        let outcome = reconcile_now(&ctx);
        match outcome {
            ActionOutcome::Conflict {
                owner_pid, surface, ..
            } => {
                assert_eq!(surface, "writer lock");
                assert_eq!(owner_pid, Some(99999));
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn start_watch_daemon_on_ready_repo_acknowledges_and_observes_running() {
        let _guard = crate::test_support::global_test_lock("tui-start-watch-daemon");
        let (_home, _home_guard) = isolated_home();
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());

        let start = start_watch_daemon(&ctx);
        assert!(matches!(start, ActionOutcome::Ack { .. }), "got {start:?}");
        assert!(
            matches!(
                crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir),
                crate::pipeline::watch::WatchServiceStatus::Running(_)
            ),
            "watch should be running after start"
        );

        let stop = stop_watch(&ctx);
        assert!(
            matches!(
                stop,
                ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
            ),
            "cleanup stop must succeed, got {stop:?}"
        );
        assert!(
            matches!(
                crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir),
                crate::pipeline::watch::WatchServiceStatus::Inactive
            ),
            "watch should be inactive after stop"
        );
    }

    #[test]
    fn start_watch_daemon_reports_conflict_when_already_running() {
        let _guard = crate::test_support::global_test_lock("tui-start-watch-daemon");
        let (_home, _home_guard) = isolated_home();
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());

        let first = start_watch_daemon(&ctx);
        assert!(matches!(first, ActionOutcome::Ack { .. }), "got {first:?}");
        let second = start_watch_daemon(&ctx);
        assert!(
            matches!(second, ActionOutcome::Conflict { .. }),
            "second start should conflict, got {second:?}"
        );

        let stop = stop_watch(&ctx);
        assert!(
            matches!(
                stop,
                ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
            ),
            "cleanup stop must succeed, got {stop:?}"
        );
    }

    #[test]
    fn start_watch_daemon_on_uninitialized_repo_returns_error() {
        let dir = tempdir().unwrap();
        let ctx = ActionContext::new(dir.path());
        let outcome = start_watch_daemon(&ctx);
        assert!(
            matches!(outcome, ActionOutcome::Error { .. }),
            "expected Error, got {outcome:?}"
        );
    }
}
