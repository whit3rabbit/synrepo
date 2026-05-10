use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    config::Config,
    pipeline::watch::{lease::WatchStateHandle, reconcile::ReconcileOutcome},
    substrate::embedding::is_available,
};

use super::job::{run_auto_embedding_refresh, EmbeddingJobContext};

const DEFAULT_QUIET_WINDOW: Duration = Duration::from_secs(30);
const DEFAULT_FAILURE_BACKOFF: Duration = Duration::from_secs(5 * 60);

pub(in crate::pipeline::watch) struct EmbeddingRefreshScheduler {
    stale: bool,
    quiet_until: Option<Instant>,
    backoff_until: Option<Instant>,
    handle: Option<thread::JoinHandle<AutoRefreshOutcome>>,
    quiet_window: Duration,
    failure_backoff: Duration,
}

pub(in crate::pipeline::watch) struct ReconcileEmbeddingObservation<'a> {
    pub(in crate::pipeline::watch) outcome: &'a ReconcileOutcome,
    pub(in crate::pipeline::watch) triggering_events: usize,
    pub(in crate::pipeline::watch) force_full_reconcile: bool,
    pub(in crate::pipeline::watch) keepalive: bool,
}

enum AutoRefreshOutcome {
    Completed,
    Skipped,
    Failed(String),
}

impl Default for EmbeddingRefreshScheduler {
    fn default() -> Self {
        Self::new(DEFAULT_QUIET_WINDOW, DEFAULT_FAILURE_BACKOFF)
    }
}

impl EmbeddingRefreshScheduler {
    fn new(quiet_window: Duration, failure_backoff: Duration) -> Self {
        Self {
            stale: false,
            quiet_until: None,
            backoff_until: None,
            handle: None,
            quiet_window,
            failure_backoff,
        }
    }

    pub(in crate::pipeline::watch) fn note_reconcile(
        &mut self,
        config: &Config,
        synrepo_dir: &Path,
        observation: ReconcileEmbeddingObservation<'_>,
        state_handle: &WatchStateHandle,
    ) {
        if observation.keepalive
            || !matches!(observation.outcome, ReconcileOutcome::Completed(_))
            || !(observation.triggering_events > 0 || observation.force_full_reconcile)
            || !existing_index_can_refresh(config, synrepo_dir)
        {
            return;
        }

        self.stale = true;
        self.quiet_until = Some(Instant::now() + self.quiet_window);
        state_handle.note_embedding_stale(true);
    }

    pub(in crate::pipeline::watch) fn reap_finished(&mut self, state_handle: &WatchStateHandle) {
        let Some(handle) = self.handle.as_ref() else {
            return;
        };
        if !handle.is_finished() {
            return;
        }
        let handle = self.handle.take().expect("checked above");
        match handle.join() {
            Ok(AutoRefreshOutcome::Completed | AutoRefreshOutcome::Skipped) => {
                self.stale = false;
                self.quiet_until = None;
                self.backoff_until = None;
            }
            Ok(AutoRefreshOutcome::Failed(_message)) => {
                let retry_at = Instant::now() + self.failure_backoff;
                self.backoff_until = Some(retry_at);
                state_handle.note_embedding_retry_after(rfc3339_after(self.failure_backoff));
            }
            Err(_) => {
                let retry_at = Instant::now() + self.failure_backoff;
                self.backoff_until = Some(retry_at);
                state_handle.note_embedding_error("embedding refresh thread panicked");
                state_handle.note_embedding_retry_after(rfc3339_after(self.failure_backoff));
            }
        }
    }

    pub(in crate::pipeline::watch) fn is_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    pub(in crate::pipeline::watch) fn clear_stale(&mut self, state_handle: &WatchStateHandle) {
        self.stale = false;
        self.quiet_until = None;
        self.backoff_until = None;
        state_handle.note_embedding_stale(false);
    }

    pub(in crate::pipeline::watch) fn maybe_start_auto_refresh(
        &mut self,
        context: EmbeddingJobContext,
        auto_sync_enabled: &AtomicBool,
        auto_sync_blocked: &AtomicBool,
        pending_changes: bool,
    ) -> bool {
        self.reap_finished(&context.state_handle);
        if !self.should_start_auto_refresh(
            &context.config,
            &context.synrepo_dir,
            auto_sync_enabled,
            auto_sync_blocked,
            pending_changes,
            Instant::now(),
        ) {
            return false;
        }

        let state_handle = context.state_handle.clone();
        let handle = match thread::Builder::new()
            .name("synrepo-embedding-refresh".to_string())
            .spawn(move || match run_auto_embedding_refresh(context) {
                Ok(Some(_)) => AutoRefreshOutcome::Completed,
                Ok(None) => AutoRefreshOutcome::Skipped,
                Err(err) => AutoRefreshOutcome::Failed(err.to_string()),
            }) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::warn!(error = %err, "failed to spawn embedding refresh thread");
                let retry_at = Instant::now() + self.failure_backoff;
                self.backoff_until = Some(retry_at);
                state_handle.note_embedding_error(format!(
                    "failed to spawn embedding refresh thread: {err}"
                ));
                state_handle.note_embedding_retry_after(rfc3339_after(self.failure_backoff));
                return false;
            }
        };
        self.handle = Some(handle);
        true
    }

    pub(in crate::pipeline::watch) fn join_on_stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    fn should_start_auto_refresh(
        &self,
        config: &Config,
        synrepo_dir: &Path,
        auto_sync_enabled: &AtomicBool,
        auto_sync_blocked: &AtomicBool,
        pending_changes: bool,
        now: Instant,
    ) -> bool {
        self.stale
            && !pending_changes
            && !self.is_running()
            && self.quiet_until.is_none_or(|due| now >= due)
            && self.backoff_until.is_none_or(|due| now >= due)
            && auto_sync_enabled.load(Ordering::Relaxed)
            && !auto_sync_blocked.load(Ordering::Relaxed)
            && existing_index_can_refresh(config, synrepo_dir)
    }
}

fn existing_index_can_refresh(config: &Config, synrepo_dir: &Path) -> bool {
    is_available()
        && config.enable_semantic_triage
        && synrepo_dir.join("index/vectors/index.bin").exists()
}

fn rfc3339_after(duration: Duration) -> String {
    let duration = time::Duration::try_from(duration).unwrap_or(time::Duration::ZERO);
    (OffsetDateTime::now_utc() + duration)
        .format(&Rfc3339)
        .unwrap_or_else(|_| crate::pipeline::writer::now_rfc3339())
}

#[cfg(all(test, feature = "semantic-triage"))]
impl EmbeddingRefreshScheduler {
    pub(super) fn for_test(quiet_window: Duration, failure_backoff: Duration) -> Self {
        Self::new(quiet_window, failure_backoff)
    }

    pub(super) fn stale_for_test(&self) -> bool {
        self.stale
    }

    pub(super) fn force_backoff_for_test(&mut self, duration: Duration) {
        self.backoff_until = Some(Instant::now() + duration);
    }

    pub(super) fn should_start_for_test(
        &self,
        config: &Config,
        synrepo_dir: &Path,
        auto_sync_enabled: &AtomicBool,
        auto_sync_blocked: &AtomicBool,
        pending_changes: bool,
    ) -> bool {
        self.should_start_auto_refresh(
            config,
            synrepo_dir,
            auto_sync_enabled,
            auto_sync_blocked,
            pending_changes,
            Instant::now(),
        )
    }
}
