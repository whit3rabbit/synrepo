//! Low-level compaction operations: timestamp loading, repair-log rotation, WAL checkpoint.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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

/// Save the compaction timestamp to state file.
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
    let tmp_path = state_dir.join("compact-state.tmp");
    let mut tmp = File::create(&tmp_path)?;
    tmp.write_all(content.as_bytes())?;
    tmp.sync_all()?;
    drop(tmp);
    fs::rename(&tmp_path, state_dir.join("compact-state.json"))?;
    Ok(())
}

/// Rotate the repair-log file: summarize old entries into a header line,
/// preserve recent entries within the retention window.
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

    let file = File::open(&log_path)?;
    let reader = BufReader::new(file);

    // Single pass: collect recent entries and build summary counts simultaneously.
    let mut entries: Vec<String> = Vec::new();
    let mut summarized_count = 0;
    let mut surface_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut action_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        // Parse timestamp from JSON line to check age.
        // Expected format: {"timestamp": "...", ...}
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(ts) = entry.get("timestamp").and_then(|v| v.as_str()) {
                if let Ok(dt) =
                    OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
                {
                    if dt < cutoff {
                        // Old entry - summarize it.
                        summarized_count += 1;
                        if let Some(surface) = entry.get("surface").and_then(|v| v.as_str()) {
                            *surface_counts.entry(surface.to_string()).or_insert(0) += 1;
                        }
                        if let Some(action) = entry.get("action").and_then(|v| v.as_str()) {
                            *action_counts.entry(action.to_string()).or_insert(0) += 1;
                        }
                        continue; // Skip this entry (will be summarized)
                    }
                }
            }
        }
        // Recent entry - retain it.
        entries.push(line);
    }

    // Write new file with summary header + retained entries.
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

    let tmp_path = log_path.with_extension("jsonl.tmp");
    {
        let mut tmp = File::create(&tmp_path)?;
        writeln!(tmp, "{}", header)?;
        for entry in &entries {
            writeln!(tmp, "{}", entry)?;
        }
        tmp.sync_all()?;
    }
    fs::rename(&tmp_path, &log_path)?;

    let summary = crate::pipeline::maintenance::CompactSummary {
        repair_log_summarized: summarized_count,
        compaction_timestamp: OffsetDateTime::now_utc(),
        ..Default::default()
    };
    Ok(summary)
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
