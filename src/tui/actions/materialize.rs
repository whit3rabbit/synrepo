//! `materialize_now` action dispatcher.
//!
//! Bridges the dashboard's `M` key (and the auto-fire path inside `tick()`)
//! to the [`MaterializerSupervisor`]. Performs an upstream watch-status
//! check so the dashboard returns a structured [`ActionOutcome::Conflict`]
//! when the watch service holds the writer lease, instead of letting
//! `bootstrap()` race the watcher and surface a generic anyhow error.
//!
//! Like [`super::reconcile::reconcile_now`], this never bypasses the
//! writer lock: `bootstrap()` acquires it internally inside the supervisor
//! thread.

use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::materializer::MaterializerSupervisor;

use super::{ActionContext, ActionOutcome};

/// Dispatch a graph-materialization run on behalf of the dashboard.
///
/// Returns immediately. The supervisor's background thread does the work and
/// posts the outcome via [`MaterializerSupervisor::try_drain`], which the
/// dashboard polls each tick.
pub fn materialize_now(
    ctx: &ActionContext,
    supervisor: &mut MaterializerSupervisor,
) -> ActionOutcome {
    match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(state) => ActionOutcome::Conflict {
            owner_pid: Some(state.pid),
            acquired_at: None,
            surface: "watch lease".to_string(),
            guidance: format!(
                "watch service (pid {}) owns this repo; stop it (w) before generating the graph",
                state.pid
            ),
        },
        WatchServiceStatus::Starting => ActionOutcome::Conflict {
            owner_pid: None,
            acquired_at: None,
            surface: "watch lease".to_string(),
            guidance: "watch service is still starting; wait for it to settle, or stop it before generating the graph"
                .to_string(),
        },
        WatchServiceStatus::Inactive
        | WatchServiceStatus::Stale(_)
        | WatchServiceStatus::Corrupt(_) => match supervisor.start() {
            Ok(()) => ActionOutcome::Ack {
                message: "generating graph...".to_string(),
            },
            Err(_already_running) => ActionOutcome::Ack {
                message: "graph materialization already in progress".to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    fn isolated_home() -> (tempfile::TempDir, crate::config::test_home::HomeEnvGuard) {
        let home = tempfile::tempdir().unwrap();
        let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        (home, guard)
    }

    fn init_repo(path: &Path) {
        crate::bootstrap::bootstrap(path, None, false).expect("bootstrap");
    }

    #[test]
    fn materialize_now_on_uninitialized_repo_starts_supervisor() {
        let _guard = crate::test_support::global_test_lock("tui-materialize-now");
        let (_home, _home_guard) = isolated_home();
        let dir = tempdir().unwrap();
        let ctx = ActionContext::new(dir.path());
        let mut sup = MaterializerSupervisor::new(dir.path());

        let outcome = materialize_now(&ctx, &mut sup);
        assert!(
            matches!(outcome, ActionOutcome::Ack { .. }),
            "got {outcome:?}"
        );
        assert!(sup.is_running());

        // Drain so the supervisor thread does not leak into the next test.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while sup.try_drain().is_none() {
            if std::time::Instant::now() >= deadline {
                panic!("materializer did not finish within 30s");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn materialize_now_under_active_watch_returns_conflict() {
        let _guard = crate::test_support::global_test_lock("tui-materialize-now");
        let (_home, _home_guard) = isolated_home();
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());

        let start = super::super::start_watch_daemon(&ctx);
        assert!(matches!(start, ActionOutcome::Ack { .. }), "got {start:?}");

        let mut sup = MaterializerSupervisor::new(dir.path());
        let outcome = materialize_now(&ctx, &mut sup);
        match &outcome {
            ActionOutcome::Conflict { surface, .. } => {
                assert_eq!(surface, "watch lease", "got {outcome:?}");
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
        assert!(
            !sup.is_running(),
            "supervisor must not start under watch lease"
        );

        let stop = super::super::stop_watch(&ctx);
        assert!(
            matches!(
                stop,
                ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
            ),
            "cleanup stop must succeed, got {stop:?}"
        );
    }

    #[test]
    fn materialize_now_when_already_running_returns_ack() {
        let _guard = crate::test_support::global_test_lock("tui-materialize-now");
        let (_home, _home_guard) = isolated_home();
        let dir = tempdir().unwrap();
        let ctx = ActionContext::new(dir.path());
        let mut sup = MaterializerSupervisor::new(dir.path());

        let first = materialize_now(&ctx, &mut sup);
        assert!(matches!(first, ActionOutcome::Ack { .. }), "got {first:?}");
        let second = materialize_now(&ctx, &mut sup);
        assert!(
            matches!(second, ActionOutcome::Ack { .. }),
            "double dispatch must Ack, got {second:?}"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while sup.try_drain().is_none() {
            if std::time::Instant::now() >= deadline {
                panic!("materializer did not finish within 30s");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
