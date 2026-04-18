//! Mapping from `WatchEvent` (live-mode bus) to the log-pane `LogEntry`.
//! Lives in its own module so the tag/severity contract can be pinned by
//! unit tests without constructing an `AppState`.

use crate::pipeline::watch::{ReconcileOutcome, WatchEvent};
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

/// Pure mapping from a `WatchEvent` to a `LogEntry`.
pub fn watch_event_to_log_entry(event: WatchEvent) -> LogEntry {
    let tag = "watch".to_string();
    match event {
        WatchEvent::ReconcileStarted {
            at,
            triggering_events,
        } => LogEntry {
            timestamp: at,
            tag,
            message: if triggering_events == 0 {
                "reconcile started".to_string()
            } else {
                format!("reconcile started ({triggering_events} events)")
            },
            severity: Severity::Healthy,
        },
        WatchEvent::ReconcileFinished {
            at,
            outcome,
            triggering_events: _,
        } => {
            let (message, severity) = match outcome {
                ReconcileOutcome::Completed(summary) => (
                    format!(
                        "reconcile finished ({} files, {} symbols)",
                        summary.files_discovered, summary.symbols_extracted
                    ),
                    Severity::Healthy,
                ),
                ReconcileOutcome::LockConflict { holder_pid } => (
                    format!("reconcile deferred: writer lock held by pid {holder_pid}"),
                    Severity::Stale,
                ),
                ReconcileOutcome::Failed(message) => {
                    (format!("reconcile failed: {message}"), Severity::Blocked)
                }
            };
            LogEntry {
                timestamp: at,
                tag,
                message,
                severity,
            }
        }
        WatchEvent::Error { at, message } => LogEntry {
            timestamp: at,
            tag,
            message: format!("watch error: {message}"),
            severity: Severity::Blocked,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_started_maps_to_healthy_watch_entry() {
        let entry = watch_event_to_log_entry(WatchEvent::ReconcileStarted {
            at: "2026-04-17T22:00:00Z".to_string(),
            triggering_events: 0,
        });
        assert_eq!(entry.tag, "watch");
        assert_eq!(entry.timestamp, "2026-04-17T22:00:00Z");
        assert_eq!(entry.message, "reconcile started");
        assert!(matches!(entry.severity, Severity::Healthy));
    }

    #[test]
    fn reconcile_finished_completed_maps_to_healthy_counts() {
        use crate::pipeline::structural::CompileSummary;
        let summary = CompileSummary {
            files_discovered: 12,
            symbols_extracted: 34,
            ..Default::default()
        };
        let entry = watch_event_to_log_entry(WatchEvent::ReconcileFinished {
            at: "2026-04-17T22:00:01Z".to_string(),
            outcome: ReconcileOutcome::Completed(summary),
            triggering_events: 3,
        });
        assert_eq!(entry.tag, "watch");
        assert_eq!(entry.message, "reconcile finished (12 files, 34 symbols)");
        assert!(matches!(entry.severity, Severity::Healthy));
    }

    #[test]
    fn reconcile_lock_conflict_maps_to_stale() {
        let entry = watch_event_to_log_entry(WatchEvent::ReconcileFinished {
            at: "2026-04-17T22:00:02Z".to_string(),
            outcome: ReconcileOutcome::LockConflict { holder_pid: 4242 },
            triggering_events: 0,
        });
        assert_eq!(entry.tag, "watch");
        assert!(entry.message.contains("4242"));
        assert!(matches!(entry.severity, Severity::Stale));
    }

    #[test]
    fn reconcile_failed_maps_to_blocked() {
        let entry = watch_event_to_log_entry(WatchEvent::ReconcileFinished {
            at: "2026-04-17T22:00:03Z".to_string(),
            outcome: ReconcileOutcome::Failed("boom".to_string()),
            triggering_events: 0,
        });
        assert_eq!(entry.tag, "watch");
        assert!(entry.message.contains("boom"));
        assert!(matches!(entry.severity, Severity::Blocked));
    }

    #[test]
    fn error_maps_to_blocked() {
        let entry = watch_event_to_log_entry(WatchEvent::Error {
            at: "2026-04-17T22:00:04Z".to_string(),
            message: "debouncer crashed".to_string(),
        });
        assert_eq!(entry.tag, "watch");
        assert!(entry.message.contains("debouncer crashed"));
        assert!(matches!(entry.severity, Severity::Blocked));
    }
}
