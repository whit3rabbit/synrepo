use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    config::Config,
    pipeline::watch::{lease::WatchStateHandle, reconcile::ReconcileOutcome},
    substrate::embedding::is_available,
};

const DEFAULT_QUIET_WINDOW: Duration = Duration::from_secs(30);
const DEFAULT_FAILURE_BACKOFF: Duration = Duration::from_secs(5 * 60);

pub(in crate::pipeline::watch) struct EmbeddingRefreshScheduler {
    stale: bool,
    quiet_until: Option<Instant>,
    backoff_until: Option<Instant>,
    quiet_window: Duration,
    failure_backoff: Duration,
}

pub(in crate::pipeline::watch) struct ReconcileEmbeddingObservation<'a> {
    pub(in crate::pipeline::watch) outcome: &'a ReconcileOutcome,
    pub(in crate::pipeline::watch) triggering_events: usize,
    pub(in crate::pipeline::watch) keepalive: bool,
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
        let ReconcileOutcome::Completed(summary) = observation.outcome else {
            return;
        };
        if observation.keepalive
            || !(observation.triggering_events > 0 || summary.graph_changed())
            || !existing_index_can_refresh(config, synrepo_dir)
        {
            return;
        }

        self.stale = true;
        self.quiet_until = Some(Instant::now() + self.quiet_window);
        state_handle.note_embedding_stale(true);
    }

    pub(in crate::pipeline::watch) fn note_auto_refresh_finished(
        &mut self,
        result: &Result<(), String>,
        state_handle: &WatchStateHandle,
    ) {
        match result {
            Ok(()) => {
                self.stale = false;
                self.quiet_until = None;
                self.backoff_until = None;
            }
            Err(message) => {
                let retry_at = Instant::now() + self.failure_backoff;
                self.backoff_until = Some(retry_at);
                state_handle.note_embedding_error(message.clone());
                state_handle.note_embedding_retry_after(rfc3339_after(self.failure_backoff));
            }
        }
    }

    pub(in crate::pipeline::watch) fn clear_stale(&mut self, state_handle: &WatchStateHandle) {
        self.stale = false;
        self.quiet_until = None;
        self.backoff_until = None;
        state_handle.note_embedding_stale(false);
    }

    pub(in crate::pipeline::watch) fn should_start_auto_refresh(
        &self,
        config: &Config,
        synrepo_dir: &Path,
        auto_sync_enabled: &AtomicBool,
        auto_sync_blocked: &AtomicBool,
        pending_changes: bool,
    ) -> bool {
        self.should_start_auto_refresh_at(
            config,
            synrepo_dir,
            auto_sync_enabled,
            auto_sync_blocked,
            pending_changes,
            Instant::now(),
        )
    }

    fn should_start_auto_refresh_at(
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
        // Computes the same subdirectory the explicit build path writes to;
        // decides whether a previous build is available to refresh in place.
        && crate::substrate::embedding::profile_index_path_for_config(synrepo_dir, config)
            .exists()
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
        self.should_start_auto_refresh_at(
            config,
            synrepo_dir,
            auto_sync_enabled,
            auto_sync_blocked,
            pending_changes,
            Instant::now(),
        )
    }
}
