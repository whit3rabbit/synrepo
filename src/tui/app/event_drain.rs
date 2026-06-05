use crossbeam_channel::TryRecvError;

use super::AppState;
use crate::pipeline::explain::telemetry;
use crate::pipeline::watch::WatchEvent;

impl AppState {
    pub(super) fn drain_watch_events(&mut self) {
        let Some(rx) = self.events_rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    match &event {
                        WatchEvent::ReconcileStarted { .. } | WatchEvent::SyncStarted { .. } => {
                            self.reconcile_active = true
                        }
                        WatchEvent::ReconcileFinished { .. }
                        | WatchEvent::SyncFinished { .. }
                        | WatchEvent::Error { .. } => self.reconcile_active = false,
                        WatchEvent::SyncProgress { .. }
                        | WatchEvent::EmbeddingStarted { .. }
                        | WatchEvent::EmbeddingProgress { .. }
                        | WatchEvent::EmbeddingFinished { .. } => {}
                    }
                    self.log.push(super::watch_event_to_log_entry(event));
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.events_rx = None;
                    self.reconcile_active = false;
                    return;
                }
            }
        }
    }

    pub(super) fn drain_explain_events(&mut self) {
        loop {
            match self.explain_rx.try_recv() {
                Ok(event) => {
                    if let Some(entry) = super::explain_event_to_log_entry(event) {
                        self.log.push(entry);
                    }
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // Re-subscribe so a dropped or reaped sender does not
                    // silently stop the feed. Telemetry fanout reaps
                    // disconnected receivers on every publish, so we may land
                    // here after a long idle period.
                    self.explain_rx = telemetry::subscribe();
                    return;
                }
            }
        }
    }
}
