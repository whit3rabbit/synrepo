use std::path::{Path, PathBuf};

use serde_json::json;

use crate::pipeline::{
    watch::{request_watch_control, watch_service_status, WatchControlRequest, WatchServiceStatus},
    writer::LockError,
};

const EDIT_SUPPRESSION_TTL_MS: u64 = 15_000;

pub(super) fn suppress_watch_events(synrepo_dir: &Path, paths: &[PathBuf]) {
    if !matches!(
        watch_service_status(synrepo_dir),
        WatchServiceStatus::Running(_)
    ) {
        return;
    }
    // Suppress only the exact edited file paths. Atomic-save temp-sibling
    // noise (.a.rs.tmp.xxx) is already handled by `is_atomic_write_temp_sibling`
    // in suppression.rs. Adding the parent directory would silently suppress
    // unrelated sibling files edited by the user during the TTL window.
    let _ = request_watch_control(
        synrepo_dir,
        WatchControlRequest::SuppressPaths {
            paths: paths.to_vec(),
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
