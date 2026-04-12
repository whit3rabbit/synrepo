use std::{io::Write as _, path::Path, path::PathBuf};

use super::ResolutionLogEntry;

const REPAIR_LOG_FILENAME: &str = "repair-log.jsonl";

/// Canonical path of the repair resolution log.
pub fn repair_log_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(REPAIR_LOG_FILENAME)
}

/// Append one resolution log entry to `.synrepo/state/repair-log.jsonl`.
///
/// Never blocks the caller; I/O failures are logged as warnings rather than
/// propagated so that a broken filesystem does not block a repair run.
pub fn append_resolution_log(synrepo_dir: &Path, entry: &ResolutionLogEntry) {
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    let state_dir = synrepo_dir.join("state");
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        tracing::warn!(path = ?state_dir, error = %e, "failed to create state dir for repair log");
        return;
    }
    let log_path = repair_log_path(synrepo_dir);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Err(e) => {
            tracing::warn!(path = ?log_path, error = %e, "failed to open repair log for append");
        }
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::warn!(path = ?log_path, error = %e, "failed to write repair log entry");
            }
        }
    }
}
