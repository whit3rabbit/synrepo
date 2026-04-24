use crate::pipeline::watch::{
    request_watch_control, WatchControlRequest, WatchControlResponse, WatchServiceStatus,
};

use super::{ActionContext, ActionOutcome};

/// Flip the in-memory auto-sync flag on the running watch service.
///
/// `desired` is the new value. Callers typically pass `!current_state` where
/// `current_state` is tracked in the dashboard. The watch service accepts the
/// control message in both `on` and `off` directions; the ack carries the
/// resulting state for the caller to confirm.
pub fn set_auto_sync(ctx: &ActionContext, desired: bool) -> ActionOutcome {
    match crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(state) => match request_watch_control(
            &ctx.synrepo_dir,
            WatchControlRequest::SetAutoSync { enabled: desired },
        ) {
            Ok(WatchControlResponse::Ack { message }) => ActionOutcome::Ack { message },
            Ok(WatchControlResponse::Error { message }) => ActionOutcome::Error { message },
            Ok(_) => ActionOutcome::Error {
                message: format!(
                    "watch service (pid {}) returned an unexpected response to set-auto-sync",
                    state.pid
                ),
            },
            Err(err) => ActionOutcome::Error {
                message: format!("set-auto-sync delegate failed: {err}"),
            },
        },
        _ => ActionOutcome::Error {
            message: "auto-sync toggle requires an active watch service (start with `w`)"
                .to_string(),
        },
    }
}
