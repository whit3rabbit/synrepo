use std::path::Path;

use serde_json::json;

use crate::{
    pipeline::{
        watch::{
            request_watch_control, watch_service_status, WatchControlRequest, WatchServiceStatus,
        },
        writer::LockError,
    },
    surface::mcp::SynrepoState,
};

const EDIT_SUPPRESSION_TTL_MS: u64 = 15_000;

pub(super) fn suppress_watch_events(state: &SynrepoState, synrepo_dir: &Path, paths: &[String]) {
    if !matches!(
        watch_service_status(synrepo_dir),
        WatchServiceStatus::Running(_)
    ) {
        return;
    }
    let mut watch_paths = Vec::with_capacity(paths.len() * 2);
    for path in paths {
        let abs_path = state.repo_root.join(path);
        watch_paths.push(abs_path.clone());
        if let Some(parent) = abs_path.parent() {
            watch_paths.push(parent.to_path_buf());
        }
    }
    let _ = request_watch_control(
        synrepo_dir,
        WatchControlRequest::SuppressPaths {
            paths: watch_paths,
            ttl_ms: EDIT_SUPPRESSION_TTL_MS,
        },
    );
}

pub(super) fn writer_lock_conflict_json(err: LockError) -> serde_json::Value {
    match err {
        LockError::HeldByOther { pid, lock_path } => json!({
            "status": "writer_lock_conflict",
            "writer_lock": {
                "holder_pid": pid,
                "path": lock_path,
            },
            "files": [],
        }),
        other => json!({
            "status": "writer_lock_conflict",
            "writer_lock": {
                "message": other.to_string(),
            },
            "files": [],
        }),
    }
}
