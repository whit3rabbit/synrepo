//! Low-level compaction operations: timestamp loading, repair-log rotation, WAL checkpoint.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::util::atomic_write::atomic_write;

/// Subset of a repair-log JSONL line we read during rotation. Typed
/// deserialization is roughly an order of magnitude cheaper than `Value`
/// for logs that may grow to millions of lines.
#[derive(Default, Deserialize)]
struct LogLineView {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    action: Option<String>,
}

/// State file content for compaction tracking.
#[derive(Deserialize, Serialize)]
pub(crate) struct CompactState {
    pub(crate) last_compaction_timestamp: Option<OffsetDateTime>,
}

/// Load the last compaction timestamp from state file.
pub fn load_last_compaction_timestamp(synrepo_dir: &Path) -> Option<OffsetDateTime> {
    let state_file = synrepo_dir.join("state/compact-state.json");
    if !state_file.exists() {
        return None;
    }
    let content = fs::read_to_string(&state_file).ok()?;
    let state: CompactState = serde_json::from_str(&content).ok()?;
    state.last_compaction_timestamp
}

/// Save the compaction timestamp to state file via the atomic-write helper,
/// which fsyncs both the temp file and the parent directory on Unix so the
/// rename is durable across crashes.
pub(super) fn save_compaction_timestamp(
    synrepo_dir: &Path,
    timestamp: OffsetDateTime,
) -> crate::Result<()> {
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir)?;
    let state = CompactState {
        last_compaction_timestamp: Some(timestamp),
    };
    let content = serde_json::to_string_pretty(&state)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("serialize compact state: {e}")))?;
    atomic_write(&state_dir.join("compact-state.json"), content.as_bytes())?;
    Ok(())
}

/// Rotate the repair-log file: summarize old entries into a header line,
/// preserve recent entries within the retention window.
///
/// Streams the input twice (once to count summarized entries, once to copy
/// survivors into a temp file). Memory use stays constant in the retained-
/// entry count: the previous implementation buffered all survivors in a
/// `Vec<String>`, which OOMs on pathologically large logs.
pub fn rotate_repair_log(
    synrepo_dir: &Path,
    policy: &crate::pipeline::maintenance::CompactPolicy,
) -> crate::Result<crate::pipeline::maintenance::CompactSummary> {
    let log_path = synrepo_dir.join("state/repair-log.jsonl");
    if !log_path.exists() {
        return Ok(crate::pipeline::maintenance::CompactSummary::default());
    }

    let retention_days = policy.repair_log_retention_days();
    let cutoff = OffsetDateTime::now_utc() - time::Duration::days(retention_days as i64);

    // Open once and seek between passes so concurrent log writers can't
    // produce inconsistent counts vs. retained body.
    let mut file = File::open(&log_path)?;

    let mut summarized_count = 0usize;
    let mut surface_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut action_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    {
        let reader = BufReader::new(&file);
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            let view: LogLineView = serde_json::from_str(&line).unwrap_or_default();
            let Some(ts) = view.timestamp.as_deref() else {
                continue;
            };
            let Ok(dt) = OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
            else {
                continue;
            };
            if dt >= cutoff {
                continue;
            }
            summarized_count += 1;
            if let Some(s) = view.surface {
                *surface_counts.entry(s).or_insert(0) += 1;
            }
            if let Some(a) = view.action {
                *action_counts.entry(a).or_insert(0) += 1;
            }
        }
    }

    let mut summary_parts = Vec::new();
    if !surface_counts.is_empty() {
        summary_parts.push(format!("summarized:{}entries", summarized_count));
        for (surface, count) in &surface_counts {
            summary_parts.push(format!("{}={}", surface, count));
        }
    }
    let header = if summary_parts.is_empty() {
        format!("# compacted {} entries", summarized_count)
    } else {
        format!("# {} | {}", summarized_count, summary_parts.join(", "))
    };

    file.seek(SeekFrom::Start(0))?;
    let tmp_path = log_path.with_extension("jsonl.tmp");
    {
        let mut tmp = File::create(&tmp_path)?;
        writeln!(tmp, "{}", header)?;
        let reader = BufReader::new(&file);
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() || line_is_too_old(&line, cutoff) {
                continue;
            }
            writeln!(tmp, "{}", line)?;
        }
        tmp.sync_all()?;
    }
    fs::rename(&tmp_path, &log_path)?;

    #[cfg(unix)]
    {
        if let Some(parent) = log_path.parent() {
            if let Err(e) = File::open(parent).and_then(|d| d.sync_all()) {
                tracing::warn!(error = %e, "parent-dir fsync after repair-log rotation failed");
            }
        }
    }

    let summary = crate::pipeline::maintenance::CompactSummary {
        repair_log_summarized: summarized_count,
        compaction_timestamp: OffsetDateTime::now_utc(),
        ..Default::default()
    };
    Ok(summary)
}

/// Whether a JSONL repair-log line carries a `timestamp` older than `cutoff`.
/// Lines without a parseable timestamp are treated as "not too old" so they're
/// retained; truncating malformed lines silently is worse than carrying them.
fn line_is_too_old(line: &str, cutoff: OffsetDateTime) -> bool {
    let view: LogLineView = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let Some(ts) = view.timestamp.as_deref() else {
        return false;
    };
    OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
        .map(|dt| dt < cutoff)
        .unwrap_or(false)
}

/// Run WAL checkpoint on both graph and overlay SQLite databases.
pub fn wal_checkpoint(synrepo_dir: &Path) -> crate::Result<bool> {
    use rusqlite::Connection;

    let graph_db = synrepo_dir.join("graph/nodes.db");
    let overlay_db = synrepo_dir.join("overlay/overlay.db");

    let mut success = true;

    if graph_db.exists() {
        if let Ok(conn) = Connection::open(&graph_db) {
            if let Err(e) = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)") {
                tracing::warn!(error = %e, "WAL checkpoint failed for graph db");
                success = false;
            }
        }
    }

    if overlay_db.exists() {
        if let Ok(conn) = Connection::open(&overlay_db) {
            if let Err(e) = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)") {
                tracing::warn!(error = %e, "WAL checkpoint failed for overlay db");
                success = false;
            }
        }
    }

    Ok(success)
}
