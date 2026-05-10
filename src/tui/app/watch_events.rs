//! Mapping from `WatchEvent` (live-mode bus) to the log-pane `LogEntry`.
//! Lives in its own module so the tag/severity contract can be pinned by
//! unit tests without constructing an `AppState`.

use crate::pipeline::repair::SyncProgress;
use crate::pipeline::watch::{
    EmbeddingTrigger, ReconcileOutcome, ReconcileStartReason, SyncTrigger, WatchEvent,
};
use crate::substrate::embedding::EmbeddingBuildEvent;
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

/// Pure mapping from a `WatchEvent` to a `LogEntry`.
pub fn watch_event_to_log_entry(event: WatchEvent) -> LogEntry {
    let tag = "watch".to_string();
    match event {
        WatchEvent::ReconcileStarted {
            at,
            triggering_events,
            full,
            reason,
        } => LogEntry {
            timestamp: at,
            tag,
            message: reconcile_started_message(triggering_events, full, reason),
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
        WatchEvent::SyncStarted { at, trigger } => LogEntry {
            timestamp: at,
            tag,
            message: match trigger {
                SyncTrigger::Manual => "sync started".to_string(),
                SyncTrigger::AutoPostReconcile => "auto-sync started (cheap surfaces)".to_string(),
            },
            severity: Severity::Healthy,
        },
        WatchEvent::SyncProgress { at, progress } => LogEntry {
            timestamp: at,
            tag,
            message: sync_progress_message(&progress),
            severity: Severity::Healthy,
        },
        WatchEvent::SyncFinished {
            at,
            trigger,
            summary,
        } => {
            let trigger_label = match trigger {
                SyncTrigger::Manual => "sync",
                SyncTrigger::AutoPostReconcile => "auto-sync",
            };
            let (message, severity) = if summary.blocked.is_empty() {
                (
                    format!(
                        "{trigger_label} finished ({} repaired, {} report-only)",
                        summary.repaired.len(),
                        summary.report_only.len()
                    ),
                    Severity::Healthy,
                )
            } else {
                (
                    format!(
                        "{trigger_label} finished with {} blocked finding(s)",
                        summary.blocked.len()
                    ),
                    Severity::Blocked,
                )
            };
            LogEntry {
                timestamp: at,
                tag,
                message,
                severity,
            }
        }
        WatchEvent::EmbeddingStarted { at, trigger } => LogEntry {
            timestamp: at,
            tag,
            message: format!("{} started", embedding_label(trigger)),
            severity: Severity::Healthy,
        },
        WatchEvent::EmbeddingProgress {
            at,
            trigger,
            progress,
        } => LogEntry {
            timestamp: at,
            tag,
            message: format!(
                "{}: {}",
                embedding_label(trigger),
                embedding_progress(&progress)
            ),
            severity: Severity::Healthy,
        },
        WatchEvent::EmbeddingFinished {
            at,
            trigger,
            summary,
            error,
        } => {
            let (message, severity) = match (summary, error) {
                (Some(summary), None) => (
                    format!(
                        "{} finished ({} chunks, {})",
                        embedding_label(trigger),
                        summary.chunks,
                        summary.model
                    ),
                    Severity::Healthy,
                ),
                (_, Some(error)) => (
                    format!("{} failed: {error}", embedding_label(trigger)),
                    Severity::Stale,
                ),
                (None, None) => (
                    format!("{} skipped", embedding_label(trigger)),
                    Severity::Healthy,
                ),
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

fn embedding_label(trigger: EmbeddingTrigger) -> &'static str {
    match trigger {
        EmbeddingTrigger::Manual => "embedding build",
        EmbeddingTrigger::AutoRefresh => "embedding refresh",
    }
}

fn embedding_progress(progress: &EmbeddingBuildEvent) -> String {
    match progress {
        EmbeddingBuildEvent::ResolvingModel {
            provider,
            model,
            dim,
        } => format!("resolving {provider}/{model} ({dim}d)"),
        EmbeddingBuildEvent::ModelReady {
            provider, model, ..
        } => format!("{provider}/{model} ready"),
        EmbeddingBuildEvent::InitializingBackend => "initializing backend".to_string(),
        EmbeddingBuildEvent::PreflightStarted => "running provider preflight".to_string(),
        EmbeddingBuildEvent::PreflightFinished => "provider preflight ok".to_string(),
        EmbeddingBuildEvent::ExtractingChunks => "extracting graph chunks".to_string(),
        EmbeddingBuildEvent::ChunksReady { chunks } => format!("{chunks} chunks ready"),
        EmbeddingBuildEvent::BatchFinished { current, total } => {
            format!("embedded {current}/{total} chunks")
        }
        EmbeddingBuildEvent::SavingIndex { .. } => "saving index".to_string(),
        EmbeddingBuildEvent::Finished { chunks, .. } => format!("complete ({chunks} chunks)"),
    }
}

fn reconcile_started_message(
    triggering_events: usize,
    full: bool,
    reason: Option<ReconcileStartReason>,
) -> String {
    if reason == Some(ReconcileStartReason::WatchPathOverflow) {
        return format!("reconcile started ({triggering_events} events, full: path cap exceeded)");
    }
    if triggering_events == 0 {
        if full {
            "reconcile started".to_string()
        } else {
            "reconcile started (incremental)".to_string()
        }
    } else if full {
        format!("reconcile started ({triggering_events} events, full)")
    } else {
        format!("reconcile started ({triggering_events} events)")
    }
}

fn sync_progress_message(progress: &SyncProgress) -> String {
    match progress {
        SyncProgress::SurfaceStarted { surface, action } => {
            format!("sync {}: {}", surface.as_str(), action.as_str())
        }
        SyncProgress::SurfaceFinished { surface, outcome } => {
            format!("sync {}: {:?}", surface.as_str(), outcome)
        }
        SyncProgress::CommentaryPlan {
            refresh,
            file_seeds,
            symbol_seed_candidates,
        } => format!(
            "commentary plan: {refresh} refresh, {file_seeds} file seeds, {symbol_seed_candidates} symbol seeds"
        ),
        SyncProgress::CommentaryItem {
            current,
            generated,
            target,
            message,
            retry_attempts,
            queued_for_next_run,
            ..
        } => commentary_item_message(
            *current,
            *generated,
            target.as_deref(),
            message.as_deref(),
            *retry_attempts,
            *queued_for_next_run,
        ),
        SyncProgress::CommentarySummary {
            refreshed,
            seeded,
            not_generated,
            attempted,
            stopped,
            queued_for_next_run,
            skip_reasons,
        } => format!(
            "commentary done: {refreshed} refreshed, {seeded} seeded, {not_generated} not-generated / {attempted}{}{}{}",
            if *queued_for_next_run > 0 {
                format!(", {queued_for_next_run} queued")
            } else {
                String::new()
            },
            reason_suffix(skip_reasons),
            if *stopped { " (stopped)" } else { "" }
        ),
    }
}

fn commentary_item_message(
    current: usize,
    generated: bool,
    target: Option<&str>,
    message: Option<&str>,
    retry_attempts: usize,
    queued_for_next_run: bool,
) -> String {
    let target = target
        .map(|target| format!(" {target}"))
        .unwrap_or_default();
    if generated {
        return format!("commentary target {current}{target}: generated");
    }
    let retry = if retry_attempts > 0 {
        format!(" after {retry_attempts} retry")
    } else {
        String::new()
    };
    let queued = if queued_for_next_run { " (queued)" } else { "" };
    match message {
        Some(message) => {
            format!("commentary target {current}{target}: skipped{retry}: {message}{queued}")
        }
        None => format!("commentary target {current}{target}: skipped{retry}{queued}"),
    }
}

fn reason_suffix(skip_reasons: &[(String, usize)]) -> String {
    if skip_reasons.is_empty() {
        return String::new();
    }
    let joined = skip_reasons
        .iter()
        .map(|(reason, count)| format!("{reason}={count}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" [{joined}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_started_maps_to_healthy_watch_entry() {
        let entry = watch_event_to_log_entry(WatchEvent::ReconcileStarted {
            at: "2026-04-17T22:00:00Z".to_string(),
            triggering_events: 0,
            full: true,
            reason: None,
        });
        assert_eq!(entry.tag, "watch");
        assert_eq!(entry.timestamp, "2026-04-17T22:00:00Z");
        assert_eq!(entry.message, "reconcile started");
        assert!(matches!(entry.severity, Severity::Healthy));
    }

    #[test]
    fn reconcile_started_overflow_names_full_reconcile_reason() {
        let entry = watch_event_to_log_entry(WatchEvent::ReconcileStarted {
            at: "2026-04-17T22:00:00Z".to_string(),
            triggering_events: 3,
            full: true,
            reason: Some(ReconcileStartReason::WatchPathOverflow),
        });

        assert!(entry.message.contains("full: path cap exceeded"));
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
