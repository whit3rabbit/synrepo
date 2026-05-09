use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::pipeline::writer::now_rfc3339;
use crate::util::{atomic_write::atomic_write, file_lock::exclusive_file_lock};

use super::reconcile::{ReconcileAttempt, ReconcileOutcome};

const RECONCILE_STATE_FILENAME: &str = "reconcile-state.json";
const RECONCILE_STATE_LOCK_FILENAME: &str = "reconcile-state.lock";

/// Persisted reconcile state written to `.synrepo/state/reconcile-state.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconcileState {
    /// RFC 3339 UTC timestamp of the last reconcile attempt.
    pub last_reconcile_at: String,
    /// Outcome string: "completed", "lock-conflict", or "failed".
    pub last_outcome: String,
    /// Error message when the last reconcile failed, otherwise absent.
    pub last_error: Option<String>,
    /// Number of filesystem events that triggered this pass (0 = startup).
    pub triggering_events: usize,
    /// File count from the last completed compile; absent if not completed.
    pub files_discovered: Option<usize>,
    /// Symbol count from the last completed compile; absent if not completed.
    pub symbols_extracted: Option<usize>,
}

/// Reason reconcile state could not be loaded.
#[derive(Debug, Eq, PartialEq)]
pub enum ReconcileStateError {
    /// No reconcile state file exists.
    NotFound,
    /// The state file exists but is unreadable or malformed.
    Malformed(String),
}

/// Persist a reconcile outcome to `.synrepo/state/reconcile-state.json`.
pub fn persist_reconcile_state(
    synrepo_dir: &Path,
    outcome: &ReconcileOutcome,
    triggering_events: usize,
) {
    let attempt_started_at = now_rfc3339();
    persist_reconcile_state_for_started_at(
        synrepo_dir,
        outcome,
        triggering_events,
        &attempt_started_at,
    );
}

/// Persist a reconcile attempt using the timestamp captured before lock acquisition.
pub fn persist_reconcile_attempt_state(
    synrepo_dir: &Path,
    attempt: &ReconcileAttempt,
    triggering_events: usize,
) {
    persist_reconcile_state_for_started_at(
        synrepo_dir,
        &attempt.outcome,
        triggering_events,
        &attempt.started_at,
    );
}

fn persist_reconcile_state_for_started_at(
    synrepo_dir: &Path,
    outcome: &ReconcileOutcome,
    triggering_events: usize,
    attempt_started_at: &str,
) {
    let (last_error, files_discovered, symbols_extracted) = match outcome {
        ReconcileOutcome::Completed(summary) => (
            None,
            Some(summary.files_discovered),
            Some(summary.symbols_extracted),
        ),
        ReconcileOutcome::Failed(message) => (Some(message.clone()), None, None),
        ReconcileOutcome::LockConflict { .. } => (None, None, None),
    };

    let state = ReconcileState {
        last_reconcile_at: now_rfc3339(),
        last_outcome: outcome.as_str().to_string(),
        last_error,
        triggering_events,
        files_discovered,
        symbols_extracted,
    };

    let json = match serde_json::to_string(&state) {
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(error = %error, "failed to serialize reconcile state");
            return;
        }
    };

    let state_dir = synrepo_dir.join("state");
    if let Err(error) = fs::create_dir_all(&state_dir) {
        tracing::warn!(
            path = ?state_dir,
            error = %error,
            "failed to create state dir for reconcile state"
        );
        return;
    }

    let final_path = state_dir.join(RECONCILE_STATE_FILENAME);
    let _lock = match exclusive_file_lock(&reconcile_state_lock_path(synrepo_dir)) {
        Ok(lock) => lock,
        Err(error) => {
            tracing::warn!(path = ?final_path, error = %error, "failed to lock reconcile state");
            return;
        }
    };
    if should_skip_stale_non_completed_state(synrepo_dir, outcome, attempt_started_at) {
        return;
    }
    if let Err(error) = atomic_write(&final_path, json.as_bytes()) {
        tracing::warn!(path = ?final_path, error = %error, "failed to persist reconcile state");
    }
}

/// Load the persisted reconcile state, if present and readable.
pub fn load_reconcile_state(synrepo_dir: &Path) -> Result<ReconcileState, ReconcileStateError> {
    let path = reconcile_state_path(synrepo_dir);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ReconcileStateError::NotFound
        } else {
            ReconcileStateError::Malformed(e.to_string())
        }
    })?;
    serde_json::from_str(&text).map_err(|e| ReconcileStateError::Malformed(e.to_string()))
}

/// Canonical path of the reconcile state file.
pub fn reconcile_state_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(RECONCILE_STATE_FILENAME)
}

fn reconcile_state_lock_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir
        .join("state")
        .join(RECONCILE_STATE_LOCK_FILENAME)
}

fn should_skip_stale_non_completed_state(
    synrepo_dir: &Path,
    outcome: &ReconcileOutcome,
    attempt_started_at: &str,
) -> bool {
    if matches!(outcome, ReconcileOutcome::Completed(_)) {
        return false;
    }
    let Ok(existing) = load_reconcile_state(synrepo_dir) else {
        return false;
    };
    existing.last_outcome == "completed"
        && rfc3339_after(&existing.last_reconcile_at, attempt_started_at)
}

fn rfc3339_after(left: &str, right: &str) -> bool {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    let Ok(left) = OffsetDateTime::parse(left, &Rfc3339) else {
        return false;
    };
    let Ok(right) = OffsetDateTime::parse(right, &Rfc3339) else {
        return false;
    };
    left > right
}
