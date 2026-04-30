//! CLI-side render helpers for the status command. Data-computing code lives
//! in `synrepo::surface::status_snapshot`.

use synrepo::{
    pipeline::{diagnostics::RuntimeDiagnostics, watch::WatchServiceStatus},
    surface::status_snapshot::RepairAuditState,
};

pub fn render_watch_summary(status: &WatchServiceStatus) -> String {
    match status {
        WatchServiceStatus::Inactive => "inactive".to_string(),
        WatchServiceStatus::Starting => "starting".to_string(),
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
        (_, _, WatchServiceStatus::Starting) => {
            "watch service is starting — wait for it to become ready or use `synrepo watch status`"
        }
        (ReconcileHealth::WatchStalled { .. }, _, _) => {
            "watch service is up but the graph reconcile is over an hour old — run `synrepo watch stop` then `synrepo watch` to restart it"
        }
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
