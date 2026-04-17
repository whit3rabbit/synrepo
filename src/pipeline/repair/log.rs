use std::{io::Write as _, path::Path, path::PathBuf};

use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::ResolutionLogEntry;

const REPAIR_LOG_FILENAME: &str = "repair-log.jsonl";
const REPAIR_LOG_DEGRADED_MARKER: &str = "repair-log-degraded.flag";

/// Canonical path of the repair resolution log.
pub fn repair_log_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(REPAIR_LOG_FILENAME)
}

/// Sticky marker written when an audit-log write fails; cleared on the next
/// successful write.
pub fn repair_log_degraded_marker_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(REPAIR_LOG_DEGRADED_MARKER)
}

/// Payload of `repair-log-degraded.flag`. Tolerant of older or missing fields
/// so an unreadable/partial marker still registers as degraded (see
/// `read_repair_log_degraded_marker`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairLogDegraded {
    /// RFC3339 timestamp of the most recent append failure. Empty on a
    /// legacy/partial marker the caller couldn't decode.
    pub last_failure_at: String,
    /// Short human-readable reason for the failure (e.g. `open failed: ...`,
    /// `write failed: ...`). Free-form; not intended for programmatic parsing.
    pub last_failure_reason: String,
}

/// Load the sticky degraded marker, if present. `Ok(Some)` = degraded with a
/// decoded reason, `Ok(None)` = healthy, `Err` = the marker exists but is
/// unreadable — which the caller should still treat as degraded.
pub fn read_repair_log_degraded_marker(
    synrepo_dir: &Path,
) -> std::io::Result<Option<RepairLogDegraded>> {
    let path = repair_log_degraded_marker_path(synrepo_dir);
    match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
        Ok(body) => Ok(Some(
            serde_json::from_str::<RepairLogDegraded>(&body).unwrap_or(RepairLogDegraded {
                last_failure_at: String::new(),
                last_failure_reason: "marker unreadable".to_string(),
            }),
        )),
    }
}

/// Append one resolution log entry to `.synrepo/state/repair-log.jsonl`.
///
/// Never blocks the caller; I/O failures are logged as warnings and recorded
/// as a sticky degraded marker at `.synrepo/state/repair-log-degraded.flag`
/// so `synrepo status` can surface the signal on the next invocation. A
/// successful write clears any previous marker.
pub fn append_resolution_log(synrepo_dir: &Path, entry: &ResolutionLogEntry) {
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    let state_dir = synrepo_dir.join("state");
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        tracing::warn!(path = ?state_dir, error = %e, "failed to create state dir for repair log");
        mark_repair_log_degraded(synrepo_dir, &format!("state dir create failed: {e}"));
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
            mark_repair_log_degraded(synrepo_dir, &format!("open failed: {e}"));
        }
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::warn!(path = ?log_path, error = %e, "failed to write repair log entry");
                mark_repair_log_degraded(synrepo_dir, &format!("write failed: {e}"));
            } else {
                clear_repair_log_degraded(synrepo_dir);
            }
        }
    }
}

/// Write (or overwrite) the degraded marker with the current timestamp and a
/// caller-supplied reason. Best-effort: a failure here is logged and dropped
/// because it indicates the state dir itself is unwritable, which the caller
/// already observed.
fn mark_repair_log_degraded(synrepo_dir: &Path, reason: &str) {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| String::new());
    let marker = RepairLogDegraded {
        last_failure_at: now,
        last_failure_reason: reason.to_string(),
    };
    let Ok(body) = serde_json::to_string(&marker) else {
        return;
    };
    let path = repair_log_degraded_marker_path(synrepo_dir);
    if let Err(e) = std::fs::write(&path, body) {
        tracing::warn!(path = ?path, error = %e, "failed to write repair-log degraded marker");
    }
}

fn clear_repair_log_degraded(synrepo_dir: &Path) {
    let path = repair_log_degraded_marker_path(synrepo_dir);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(path = ?path, error = %e, "failed to clear repair-log degraded marker");
        }
    }
}
