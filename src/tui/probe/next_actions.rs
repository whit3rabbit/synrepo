//! Next-action view model logic.

use std::time::Duration;

use time::OffsetDateTime;

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::pipeline::diagnostics::{EmbeddingHealth, ReconcileHealth, WriterStatus};
use crate::pipeline::watch::{WatchDaemonState, WatchServiceStatus};
use crate::surface::status_snapshot::{ExportState, StatusSnapshot};
use crate::tui::materializer::MaterializeState;

use super::{NextAction, Severity};

/// Runtime-only dashboard context for next-action wording.
#[derive(Clone, Copy, Debug)]
pub struct NextActionRuntime<'a> {
    /// Time until the dashboard will rebuild its status snapshot.
    pub snapshot_refresh_due_in: Duration,
    /// Dashboard's cached runtime auto-sync flag, if available.
    pub auto_sync_enabled: Option<bool>,
    /// Current graph materialization state, if the dashboard owns one.
    pub materialize_state: Option<&'a MaterializeState>,
    /// Current time used for elapsed labels.
    pub now: OffsetDateTime,
}

impl Default for NextActionRuntime<'_> {
    fn default() -> Self {
        Self {
            snapshot_refresh_due_in: Duration::ZERO,
            auto_sync_enabled: None,
            materialize_state: None,
            now: OffsetDateTime::now_utc(),
        }
    }
}

/// Derive next-actions from a snapshot and integration signal.
pub fn build_next_actions(
    snapshot: &StatusSnapshot,
    integration: &AgentIntegration,
) -> Vec<NextAction> {
    build_next_actions_with_context(snapshot, integration, NextActionRuntime::default())
}

/// Derive next-actions using live dashboard runtime context.
pub fn build_next_actions_with_context(
    snapshot: &StatusSnapshot,
    integration: &AgentIntegration,
    runtime: NextActionRuntime<'_>,
) -> Vec<NextAction> {
    let mut out: Vec<NextAction> = Vec::new();

    if !snapshot.initialized {
        out.push(NextAction {
            label: "Run `synrepo init` to set up this repo".to_string(),
            severity: Severity::Blocked,
        });
        return out;
    }

    if snapshot.graph_stats.is_none() {
        out.push(graph_action(runtime.materialize_state));
    }

    if let Some(d) = &snapshot.diagnostics {
        match &d.reconcile_health {
            ReconcileHealth::Stale(_) | ReconcileHealth::Unknown => {
                out.push(reconcile_action(
                    &d.watch_status,
                    &d.writer_status,
                    runtime.snapshot_refresh_due_in,
                ));
            }
            ReconcileHealth::WatchStalled { .. } => {
                out.push(NextAction {
                    label: "Watch loop appears wedged, restart with `synrepo watch stop` then `synrepo watch`"
                        .to_string(),
                    severity: Severity::Stale,
                });
            }
            ReconcileHealth::Corrupt(_) => {
                out.push(NextAction {
                    label: "Reconcile state corrupt, run `synrepo watch stop`".to_string(),
                    severity: Severity::Blocked,
                });
            }
            ReconcileHealth::Current => {}
        }
        if !d.store_guidance.is_empty() {
            out.push(store_compat_action(&d.store_guidance[0], &d.watch_status));
        }
    }

    if snapshot.export_status.state == ExportState::Stale {
        out.push(export_action(snapshot, runtime));
    }

    add_embedding_action(&mut out, snapshot, runtime);

    add_integration_action(&mut out, integration);

    if out.is_empty() {
        out.push(NextAction {
            label: "All systems healthy. Connect an agent via MCP.".to_string(),
            severity: Severity::Healthy,
        });
    }
    out
}

fn add_embedding_action(
    out: &mut Vec<NextAction>,
    snapshot: &StatusSnapshot,
    runtime: NextActionRuntime<'_>,
) {
    let Some(diag) = &snapshot.diagnostics else {
        return;
    };
    match &diag.embedding_health {
        EmbeddingHealth::Disabled => return,
        EmbeddingHealth::Degraded { reason, .. } if reason.contains("index missing") => {
            out.push(NextAction {
                label: "Embeddings enabled but no vector index, press B to build".to_string(),
                severity: Severity::Stale,
            });
            return;
        }
        EmbeddingHealth::Degraded { reason, .. } => {
            out.push(NextAction {
                label: format!("Embedding index degraded, press B to rebuild ({reason})"),
                severity: Severity::Stale,
            });
            return;
        }
        EmbeddingHealth::Available { .. } => {}
    }

    let WatchServiceStatus::Running(watch_state) = &diag.watch_status else {
        return;
    };
    if watch_state.embedding_running {
        out.push(NextAction {
            label: embedding_running_label(watch_state, runtime),
            severity: Severity::Stale,
        });
    } else if watch_state.embedding_index_stale {
        out.push(NextAction {
            label: embedding_stale_label(watch_state, runtime),
            severity: Severity::Stale,
        });
    }
}

fn embedding_running_label(
    watch_state: &WatchDaemonState,
    runtime: NextActionRuntime<'_>,
) -> String {
    let started = watch_state
        .embedding_last_started_at
        .as_deref()
        .and_then(|at| elapsed_since(at, runtime.now))
        .map(elapsed_label)
        .unwrap_or_else(|| "moments".to_string());
    if let (Some(current), Some(total)) = (
        watch_state.embedding_progress_current,
        watch_state.embedding_progress_total,
    ) {
        return format!(
            "Embedding index refresh running, started {started} ago ({current}/{total})"
        );
    }
    format!("Embedding index refresh running, started {started} ago")
}

fn embedding_stale_label(watch_state: &WatchDaemonState, runtime: NextActionRuntime<'_>) -> String {
    if let Some(retry_at) = &watch_state.embedding_next_retry_at {
        if let Some(error) = &watch_state.embedding_last_error {
            return format!("Embedding index refresh failed, backed off until {retry_at}: {error}");
        }
        return format!("Embedding index refresh backed off until {retry_at}");
    }
    let auto_enabled = runtime
        .auto_sync_enabled
        .unwrap_or(watch_state.auto_sync_enabled);
    if auto_enabled && !watch_state.auto_sync_paused {
        return "Embedding index stale, watch will refresh after the repo is quiet".to_string();
    }
    "Embedding index stale, press B to rebuild".to_string()
}

fn store_compat_action(guidance: &str, watch_status: &WatchServiceStatus) -> NextAction {
    if guidance.contains(" is blocked because ") {
        return NextAction {
            label: "Compatibility blocked, resolve manually before retrying".to_string(),
            severity: Severity::Blocked,
        };
    }

    let compat_action = if guidance.contains("needs rebuild") {
        Some("rebuild")
    } else if guidance.contains("needs invalidation")
        || guidance.contains("needs clear-and-recreate")
    {
        Some("action")
    } else {
        None
    };

    if let Some(kind) = compat_action {
        let label = match watch_status {
            WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => {
                format!("Compatibility {kind} pending, stop watch before pressing U to apply")
            }
            _ => format!("Compatibility {kind} pending, press U to apply"),
        };
        return NextAction {
            label,
            severity: Severity::Blocked,
        };
    }

    NextAction {
        label: format!("Store compat advisory: {guidance}"),
        severity: Severity::Stale,
    }
}

fn graph_action(materialize_state: Option<&MaterializeState>) -> NextAction {
    if let Some(MaterializeState::Running { started_at }) = materialize_state {
        return NextAction {
            label: format!(
                "Graph materialization running, started {} ago",
                elapsed_label(started_at.elapsed())
            ),
            severity: Severity::Stale,
        };
    }
    NextAction {
        label: "Graph not materialized, press M to generate it".to_string(),
        severity: Severity::Blocked,
    }
}

fn reconcile_action(
    watch_status: &WatchServiceStatus,
    writer_status: &WriterStatus,
    due_in: Duration,
) -> NextAction {
    match watch_status {
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => {
            let wait = countdown_label(due_in);
            let label = match writer_status {
                WriterStatus::HeldByOther { pid } => {
                    format!("Watch reconcile waiting on writer lock held by pid {pid}, checking again in {wait}")
                }
                _ => format!("Watch reconcile pending, checking again in {wait}"),
            };
            NextAction {
                label,
                severity: Severity::Stale,
            }
        }
        _ => NextAction {
            label: "Reconcile stale, press R to reconcile".to_string(),
            severity: Severity::Stale,
        },
    }
}

fn export_action(snapshot: &StatusSnapshot, runtime: NextActionRuntime<'_>) -> NextAction {
    let watch_state = snapshot
        .diagnostics
        .as_ref()
        .and_then(|d| match &d.watch_status {
            WatchServiceStatus::Running(state) => Some(state),
            _ => None,
        });
    let Some(watch_state) = watch_state else {
        return manual_export_action();
    };

    let auto_enabled = runtime
        .auto_sync_enabled
        .unwrap_or(watch_state.auto_sync_enabled);
    if !auto_enabled {
        return manual_export_action();
    }
    export_auto_action(watch_state, runtime)
}

fn export_auto_action(
    watch_state: &WatchDaemonState,
    runtime: NextActionRuntime<'_>,
) -> NextAction {
    if watch_state.auto_sync_running {
        let started = watch_state
            .auto_sync_last_started_at
            .as_deref()
            .and_then(|at| elapsed_since(at, runtime.now))
            .map(elapsed_label)
            .unwrap_or_else(|| "moments".to_string());
        return NextAction {
            label: format!("Context export refresh running, started {started} ago"),
            severity: Severity::Stale,
        };
    }
    if watch_state.auto_sync_paused {
        return NextAction {
            label: "Auto-sync paused after blocked repair, press S to inspect".to_string(),
            severity: Severity::Stale,
        };
    }
    NextAction {
        label: format!(
            "Context export refresh is automatic, checking again in {}",
            countdown_label(runtime.snapshot_refresh_due_in)
        ),
        severity: Severity::Stale,
    }
}

fn manual_export_action() -> NextAction {
    NextAction {
        label: "Context export stale, press S to refresh".to_string(),
        severity: Severity::Stale,
    }
}

fn add_integration_action(out: &mut Vec<NextAction>, integration: &AgentIntegration) {
    match integration {
        AgentIntegration::Absent => {
            out.push(NextAction {
                label: "Agent integration absent, open integration flow".to_string(),
                severity: Severity::Stale,
            });
        }
        AgentIntegration::Partial { target } => {
            out.push(NextAction {
                label: format!(
                    "Complete MCP registration for {}",
                    integration_target_label(*target)
                ),
                severity: Severity::Stale,
            });
        }
        AgentIntegration::McpOnly { target } => {
            out.push(NextAction {
                label: format!("Write agent shim for {}", integration_target_label(*target)),
                severity: Severity::Stale,
            });
        }
        AgentIntegration::Complete { .. } => {}
    }
}

fn integration_target_label(target: AgentTargetKind) -> &'static str {
    target.as_str()
}

fn countdown_label(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis == 0 {
        return "now".to_string();
    }
    let secs = millis.div_ceil(1000);
    format!("{secs}s")
}

fn elapsed_label(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        return format!("{secs}s");
    }
    format!("{}m {}s", secs / 60, secs % 60)
}

fn elapsed_since(at: &str, now: OffsetDateTime) -> Option<Duration> {
    let then = OffsetDateTime::parse(at, &time::format_description::well_known::Rfc3339).ok()?;
    (now - then).try_into().ok()
}
