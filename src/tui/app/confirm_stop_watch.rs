//! Confirm-stop-watch modal.
//!
//! Shown when the operator triggers a explain run while a watch service is
//! active. Explain must acquire the writer lock, which watch owns, so the
//! run would fail at `acquire_write_admission` with `LockError::WatchOwned`.
//! Before exiting the TUI we give the operator a chance to stop watch (the
//! only safe way to release the lock) or cancel.

use crossterm::event::{KeyCode, KeyModifiers};

use super::{AppState, ExplainMode, PendingEmbeddingBuild, PendingExplainRun};
use crate::config::Config;
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::actions::{outcome_to_log, stop_watch, ActionContext, ActionOutcome};

/// Modal state. Owned by `AppState` while the confirm prompt is visible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfirmStopWatchState {
    /// Action the operator was about to launch when we detected a running
    /// watch service. Preserved so the confirm handler can continue after the
    /// stop succeeds.
    pub pending: PendingStopWatchAction,
}

/// Action gated by the stop-watch confirmation modal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PendingStopWatchAction {
    /// Run Explain after watch stops.
    Explain(ExplainMode),
    /// Build embedding vectors after watch stops.
    BuildEmbeddings,
}

impl AppState {
    /// Gate a explain launch on watch-service status. Replaces the previous
    /// three direct assignments to `launch_explain` (picker Enter, `r`, `c`)
    /// so every path runs through the same watch check.
    pub(super) fn queue_explain(&mut self, mode: ExplainMode) {
        let synrepo_dir = Config::synrepo_dir(&self.repo_root);
        match watch_service_status(&synrepo_dir) {
            WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => {
                self.confirm_stop_watch = Some(ConfirmStopWatchState {
                    pending: PendingStopWatchAction::Explain(mode),
                });
            }
            _ => {
                self.enqueue_pending_explain(PendingExplainRun {
                    mode,
                    stopped_watch: false,
                });
            }
        }
    }

    /// Modal key handling. Returns `Some(true)` when the key was consumed, or
    /// `None` when the outer dispatch should try to handle it. Tab switches
    /// and global quit fall through so the operator is never trapped.
    pub(super) fn handle_confirm_stop_watch_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<bool> {
        if matches!(
            code,
            KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('q')
                | KeyCode::Char('1')
                | KeyCode::Char('2')
                | KeyCode::Char('3')
                | KeyCode::Char('4')
                | KeyCode::Char('5')
                | KeyCode::Char('6')
                | KeyCode::Char('7')
                | KeyCode::Char('8')
        ) {
            self.confirm_stop_watch = None;
            return None;
        }
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.confirm_stop_watch = None;
            return None;
        }

        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                let Some(pending) = self.confirm_stop_watch.take() else {
                    return Some(true);
                };
                let ctx = ActionContext::new(&self.repo_root);
                let outcome = stop_watch(&ctx);
                self.log.push(outcome_to_log("watch", &outcome));
                match &outcome {
                    ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. } => {
                        match pending.pending {
                            PendingStopWatchAction::Explain(mode) => {
                                self.enqueue_pending_explain(PendingExplainRun {
                                    mode,
                                    stopped_watch: true,
                                });
                            }
                            PendingStopWatchAction::BuildEmbeddings => {
                                self.launch_embedding_build(PendingEmbeddingBuild {
                                    stopped_watch: true,
                                });
                            }
                        }
                    }
                    ActionOutcome::Conflict { guidance, .. } => {
                        self.set_toast(format!("watch stop blocked: {guidance}"));
                        self.confirm_stop_watch = Some(pending);
                    }
                    ActionOutcome::Error { message } => {
                        self.set_toast(format!("watch stop failed: {message}"));
                        self.confirm_stop_watch = Some(pending);
                    }
                }
                Some(true)
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm_stop_watch = None;
                Some(true)
            }
            _ => Some(true),
        }
    }
}

/// Human-readable description of a pending mode, used by the render path.
pub fn describe_pending_mode(mode: &ExplainMode) -> String {
    match mode {
        ExplainMode::AllStale => "all stale commentary".to_string(),
        ExplainMode::Changed => "files changed in the last 50 commits".to_string(),
        ExplainMode::Paths(paths) => {
            if paths.is_empty() {
                "selected folders".to_string()
            } else {
                paths.join(", ")
            }
        }
        ExplainMode::Generate { scope, target } => {
            format!("{} commentary for {target}", scope.as_str())
        }
    }
}

/// Human-readable description of a pending stop-watch action.
pub fn describe_pending_stop_action(action: &PendingStopWatchAction) -> String {
    match action {
        PendingStopWatchAction::Explain(mode) => describe_pending_mode(mode),
        PendingStopWatchAction::BuildEmbeddings => "embedding vector index".to_string(),
    }
}
