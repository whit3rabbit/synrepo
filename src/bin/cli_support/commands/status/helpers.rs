//! Helper functions for status output.

use std::path::Path;

use synrepo::pipeline::{
    diagnostics::RuntimeDiagnostics,
    repair::{read_repair_log_degraded_marker, RepairLogDegraded},
    watch::WatchServiceStatus,
};

/// Sticky-marker state for the repair audit log. `Ok` means the last write
/// succeeded (or no attempt has been made yet); `Unavailable` means a prior
/// `append_resolution_log` failed and the marker has not been cleared.
pub enum RepairAuditState {
    Ok,
    Unavailable {
        last_failure_at: String,
        last_failure_reason: String,
    },
}

pub fn render_watch_summary(status: &WatchServiceStatus) -> String {
    match status {
        WatchServiceStatus::Inactive => "inactive".to_string(),
        WatchServiceStatus::Running(state) => {
            format!("{} mode (pid {})", state.mode, state.pid)
        }
        WatchServiceStatus::Stale(Some(state)) => {
            format!("stale lease from pid {}", state.pid)
        }
        WatchServiceStatus::Stale(None) => "stale watch artifacts".to_string(),
        WatchServiceStatus::Corrupt(e) => format!("corrupt ({e})"),
    }
}

pub fn load_repair_audit_state(synrepo_dir: &Path) -> RepairAuditState {
    match read_repair_log_degraded_marker(synrepo_dir) {
        Ok(None) => RepairAuditState::Ok,
        Ok(Some(RepairLogDegraded {
            last_failure_at,
            last_failure_reason,
        })) => RepairAuditState::Unavailable {
            last_failure_at,
            last_failure_reason,
        },
        Err(e) => RepairAuditState::Unavailable {
            last_failure_at: String::new(),
            last_failure_reason: format!("marker read failed: {e}"),
        },
    }
}

pub fn render_repair_audit(state: &RepairAuditState) -> String {
    match state {
        RepairAuditState::Ok => "ok".to_string(),
        RepairAuditState::Unavailable {
            last_failure_at,
            last_failure_reason,
        } => {
            if last_failure_at.is_empty() {
                format!("unavailable ({last_failure_reason})")
            } else {
                format!("unavailable (last failure at {last_failure_at}: {last_failure_reason})")
            }
        }
    }
}

pub fn next_step(diag: &RuntimeDiagnostics, graph_missing: bool) -> &'static str {
    use synrepo::pipeline::diagnostics::{ReconcileHealth, WriterStatus};

    if graph_missing {
        return "run `synrepo init` to materialize the graph";
    }
    match (
        &diag.reconcile_health,
        &diag.writer_status,
        &diag.watch_status,
    ) {
        (_, _, WatchServiceStatus::Running(_)) => {
            "watch service is active — use `synrepo watch status` for runtime details"
        }
        (ReconcileHealth::Corrupt(_), _, _) => {
            "reconcile state is corrupt — run `synrepo watch stop` to clean up and recover"
        }
        (_, WriterStatus::Corrupt(_), _) => {
            "writer lock is corrupt — remove .synrepo/state/writer.lock to recover"
        }
        (_, _, WatchServiceStatus::Corrupt(_)) => {
            "watch state is corrupt — run `synrepo watch stop` to clean up and recover"
        }
        (_, WriterStatus::HeldByOther { .. }, _) => {
            "writer lock is held — wait for the other process or verify it is still alive"
        }
        (ReconcileHealth::Unknown, _, _) => "run `synrepo reconcile` to do the first graph pass",
        (ReconcileHealth::Stale(_), _, _) => "run `synrepo reconcile` to refresh the graph",
        (ReconcileHealth::Current, _, _) => {
            "graph is current — use `synrepo graph query` or connect the MCP server"
        }
    }
}
