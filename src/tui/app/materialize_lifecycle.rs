use super::AppState;
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::actions::{materialize_now, now_rfc3339, ActionContext};
use crate::tui::materializer::MaterializeOutcome;
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

impl AppState {
    /// Reap a finished bootstrap thread, if any. Pushes a log entry + toast
    /// describing the outcome and forces a snapshot refresh on success so
    /// the health row flips on the same frame.
    pub(super) fn drain_materializer(&mut self) {
        let Some(outcome) = self.materializer.try_drain() else {
            return;
        };
        match outcome {
            MaterializeOutcome::Completed { files, symbols } => {
                let message = format!("graph materialized: {files} files, {symbols} symbols");
                self.set_toast(message.clone());
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message,
                    severity: Severity::Healthy,
                });
                // Force a refresh so the snapshot picks up the new graph
                // store immediately rather than waiting for the next tick.
                self.refresh_now();
            }
            MaterializeOutcome::Failed { error } => {
                let toast = format!("materialize failed: {error}");
                self.set_toast(toast);
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message: format!("bootstrap failed: {error}"),
                    severity: Severity::Blocked,
                });
            }
        }
    }

    /// One-shot auto-fire: when the dashboard sees `graph_stats.is_none()`
    /// and the supervisor is idle and we have not auto-attempted yet this
    /// session, dispatch `materialize_now`. The watch precheck is delegated
    /// to the action so a watch-active repo gets the same `Conflict`
    /// guidance as a manual `M` press.
    pub(super) fn maybe_auto_materialize(&mut self) {
        if self.snapshot.graph_stats.is_some() {
            return;
        }
        if self.materializer.is_running() || self.materializer.auto_was_attempted() {
            return;
        }
        // Guarded against the .synrepo/-not-initialized case (snapshot
        // surfaces that as `repo: not initialized`, a different row): only
        // attempt when bootstrap has at least a chance of succeeding without
        // user-visible setup. Bootstrap itself is idempotent for both fresh
        // and partial states, so we let it run regardless and rely on the
        // `Failed` arm above to surface any blocker.
        let ctx = ActionContext::new(&self.repo_root);
        // Skip when a foreign watch service or starting daemon owns the
        // repo: the action would just return Conflict, but we also do not
        // want to push a noisy auto-attempt log entry in that case.
        if matches!(
            watch_service_status(&ctx.synrepo_dir),
            WatchServiceStatus::Running(_) | WatchServiceStatus::Starting
        ) {
            self.materializer.mark_auto_attempted();
            return;
        }
        let outcome = materialize_now(&ctx, &mut self.materializer);
        self.materializer.mark_auto_attempted();
        // The action returns Ack on success and Conflict if the watch
        // status flipped between our check and dispatch. Both translate to
        // a single info-level log entry so the operator knows we tried.
        match outcome {
            crate::tui::actions::ActionOutcome::Ack { message } => {
                self.set_toast(format!("auto: {message}"));
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message: format!("auto: {message}"),
                    severity: Severity::Healthy,
                });
            }
            crate::tui::actions::ActionOutcome::Conflict { guidance, .. } => {
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message: format!("auto skipped: {guidance}"),
                    severity: Severity::Stale,
                });
            }
            crate::tui::actions::ActionOutcome::Completed { message }
            | crate::tui::actions::ActionOutcome::Error { message } => {
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message,
                    severity: Severity::Stale,
                });
            }
        }
    }
}
