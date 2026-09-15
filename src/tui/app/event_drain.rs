use crossbeam_channel::TryRecvError;

use super::AppState;
use crate::pipeline::explain::telemetry;
use crate::pipeline::watch::WatchEvent;

// Bound each source independently so a continuously refilled producer cannot
// keep `tick()` inside event draining forever and starve keyboard polling.
const MAX_EVENTS_PER_SOURCE_PER_TICK: usize = 64;

impl AppState {
    pub(super) fn drain_watch_events(&mut self) {
        let Some(rx) = self.events_rx.as_ref() else {
            return;
        };
        let mut refresh_graph_counts = false;
        let mut disconnected = false;
        for _ in 0..MAX_EVENTS_PER_SOURCE_PER_TICK {
            match rx.try_recv() {
                Ok(event) => {
                    match &event {
                        WatchEvent::ReconcileStarted { .. } | WatchEvent::SyncStarted { .. } => {
                            self.reconcile_active = true
                        }
                        WatchEvent::ReconcileFinished { .. }
                        | WatchEvent::SyncFinished { .. }
                        | WatchEvent::Error { .. } => {
                            self.reconcile_active = false;
                            refresh_graph_counts |= matches!(
                                &event,
                                WatchEvent::ReconcileFinished { .. }
                                    | WatchEvent::SyncFinished { .. }
                            );
                        }
                        WatchEvent::SyncProgress { .. }
                        | WatchEvent::EmbeddingStarted { .. }
                        | WatchEvent::EmbeddingProgress { .. }
                        | WatchEvent::EmbeddingFinished { .. } => {}
                    }
                    self.log.push(super::watch_event_to_log_entry(event));
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            self.events_rx = None;
            self.reconcile_active = false;
        }
        if refresh_graph_counts {
            self.request_full_snapshot_refresh(false);
        }
    }

    pub(super) fn drain_explain_events(&mut self) {
        for _ in 0..MAX_EVENTS_PER_SOURCE_PER_TICK {
            match self.explain_rx.try_recv() {
                Ok(event) => {
                    if let Some(entry) = super::explain_event_to_log_entry(event) {
                        self.log.push(entry);
                    }
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // Re-subscribe if registration failed or the fan-out was
                    // otherwise reset, so the live feed cannot silently stop.
                    self.explain_rx = telemetry::subscribe();
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::runtime_probe::AgentIntegration;
    use crate::tui::theme::Theme;

    #[test]
    fn watch_event_drain_yields_after_bounded_batch() {
        let repo = tempfile::tempdir().unwrap();
        let (tx, rx) = crossbeam_channel::bounded(MAX_EVENTS_PER_SOURCE_PER_TICK + 8);
        let mut state =
            AppState::new_live(repo.path(), Theme::plain(), AgentIntegration::Absent, rx);
        for index in 0..MAX_EVENTS_PER_SOURCE_PER_TICK + 5 {
            tx.send(WatchEvent::ReconcileStarted {
                at: format!("event-{index}"),
                triggering_events: 1,
                full: false,
                reason: None,
            })
            .unwrap();
        }

        state.drain_watch_events();
        assert_eq!(state.log.as_slice().len(), MAX_EVENTS_PER_SOURCE_PER_TICK);

        state.drain_watch_events();
        assert_eq!(
            state.log.as_slice().len(),
            MAX_EVENTS_PER_SOURCE_PER_TICK + 5
        );
    }
}
