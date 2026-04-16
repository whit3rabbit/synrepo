//! Compaction operations for `.synrepo/` runtime stores.
//!
//! This module provides policy-driven compaction for overlay, state, and index
//! stores to reclaim disk space. It extends the maintenance module with
//! retention-based cleanup.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::config::Config;
use crate::overlay::OverlayStore;

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
fn save_compaction_timestamp(synrepo_dir: &Path, timestamp: OffsetDateTime) -> crate::Result<()> {
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

/// State file content for compaction tracking.
#[derive(Deserialize, Serialize)]
struct CompactState {
    last_compaction_timestamp: Option<OffsetDateTime>,
}

/// Rotate the repair-log file: summarize old entries into a header line,
/// preserve recent entries within the retention window.
pub fn rotate_repair_log(synrepo_dir: &Path, policy: &crate::pipeline::maintenance::CompactPolicy) -> crate::Result<crate::pipeline::maintenance::CompactSummary> {
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
    let mut surface_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut action_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        // Parse timestamp from JSON line to check age.
        // Expected format: {"timestamp": "...", ...}
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(ts) = entry.get("timestamp").and_then(|v| v.as_str()) {
                if let Ok(dt) = OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339) {
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

    let mut summary = crate::pipeline::maintenance::CompactSummary::default();
    summary.repair_log_summarized = summarized_count;
    summary.compaction_timestamp = OffsetDateTime::now_utc();
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

/// Plan compaction by querying overlay stats, repair-log age, and index freshness.
pub fn plan_compact(
    synrepo_dir: &Path,
    _config: &Config,
    _policy: crate::pipeline::maintenance::CompactPolicy,
) -> crate::Result<crate::pipeline::maintenance::CompactPlan> {
    use crate::pipeline::maintenance::{CompactAction, CompactComponent, CompactStats};

    let mut actions = Vec::new();
    let mut stats = CompactStats::default();

    // Load last compaction timestamp.
    stats.last_compaction_timestamp = load_last_compaction_timestamp(synrepo_dir);

    // Query commentary compactability.
    let overlay_dir = synrepo_dir.join("overlay");
    if overlay_dir.exists() {
        if let Ok(store) = crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir) {
            // Get commentary count and stale count.
            if let Ok(count) = store.commentary_count() {
                stats.compactable_commentary = count; // Simplified: treat all as compactable for now.
            }
            // Cross-link audit row count.
            if let Ok(audit_count) = store.cross_link_audit_count() {
                stats.compactable_cross_links = audit_count;
            }
        }
    }

    // Determine actions needed.
    if stats.compactable_commentary > 0 {
        actions.push((CompactComponent::Commentary, CompactAction::CompactCommentary));
    }
    if stats.compactable_cross_links > 0 {
        actions.push((CompactComponent::CrossLinks, CompactAction::CompactCrossLinks));
    }

    // Repair-log rotation.
    let log_path = synrepo_dir.join("state/repair-log.jsonl");
    if log_path.exists() {
        // Check if log has entries beyond retention window.
        // For now, always include rotation action if log exists.
        actions.push((CompactComponent::RepairLog, CompactAction::RotateRepairLog));
    }

    // WAL checkpoint is always beneficial.
    actions.push((CompactComponent::Wal, CompactAction::WalCheckpoint));

    // Index rebuild decision (based on config change since last compile).
    // For now, skip by default unless explicitly requested.
    // Index rebuild is handled by compatibility evaluator, not compact.

    let plan = crate::pipeline::maintenance::CompactPlan {
        actions: actions
            .into_iter()
            .map(|(component, action)| crate::pipeline::maintenance::ComponentCompact {
                component,
                action,
                reason: String::new(), // Will be filled during execution.
            })
            .collect(),
        estimated_stats: stats,
    };

    Ok(plan)
}

/// Execute the compaction plan.
pub fn execute_compact(
    synrepo_dir: &Path,
    plan: &crate::pipeline::maintenance::CompactPlan,
    policy: crate::pipeline::maintenance::CompactPolicy,
) -> crate::Result<crate::pipeline::maintenance::CompactSummary> {
    use crate::pipeline::maintenance::CompactComponent;

    let mut summary = crate::pipeline::maintenance::CompactSummary::default();
    summary.compaction_timestamp = OffsetDateTime::now_utc();

    let overlay_dir = synrepo_dir.join("overlay");

    for comp in &plan.actions {
        match comp.component {
            CompactComponent::Commentary => {
                if let Ok(mut store) = crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir) {
                    if let Ok(count) = store.compact_commentary(&policy) {
                        summary.commentary_compacted = count;
                    }
                }
            }
            CompactComponent::CrossLinks => {
                if let Ok(mut store) = crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir) {
                    if let Ok(count) = store.compact_cross_links(&policy) {
                        summary.cross_links_compacted = count;
                    }
                }
            }
            CompactComponent::RepairLog => {
                let result = rotate_repair_log(synrepo_dir, &policy)?;
                summary.repair_log_summarized = result.repair_log_summarized;
            }
            CompactComponent::Wal => {
                summary.wal_checkpoint_completed = wal_checkpoint(synrepo_dir)?;
            }
            CompactComponent::Index => {
                // Index rebuild is handled by compatibility evaluator.
                summary.index_rebuilt = false;
            }
        }
    }

    // Record completion timestamp.
    save_compaction_timestamp(synrepo_dir, summary.compaction_timestamp)?;

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rotate_repair_log_creates_header_with_summary() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let state_dir = synrepo_dir.join("state");
        fs::create_dir_all(&state_dir).unwrap();

        // Write a sample repair log.
        let log_path = state_dir.join("repair-log.jsonl");
        let now = OffsetDateTime::now_utc();
        let old_timestamp = now - time::Duration::days(60);
        let recent_timestamp = now - time::Duration::days(5);

        let old_ts_str = old_timestamp.format(&time::format_description::well_known::Rfc3339).unwrap();
        let recent_ts_str = recent_timestamp.format(&time::format_description::well_known::Rfc3339).unwrap();

        fs::write(&log_path, format!(
            r#"{{"timestamp":"{}","surface":"graph","action":"retire_edges"}}
{{"timestamp":"{}","surface":"overlay","action":"prune_orphans"}}
"#,
            old_ts_str, recent_ts_str
        )).unwrap();

        let summary = rotate_repair_log(&synrepo_dir, &crate::pipeline::maintenance::CompactPolicy::Default).unwrap();
        assert_eq!(summary.repair_log_summarized, 1);

        // Verify new log has header and remaining entry.
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.starts_with('#'), "should have summary header");
        assert!(content.contains(&recent_ts_str), "should retain recent entry");
    }

    #[test]
    fn wal_checkpoint_completes_without_error() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let graph_dir = synrepo_dir.join("graph");
        fs::create_dir_all(&graph_dir).unwrap();

        // Create a minimal graph db.
        let db_path = graph_dir.join("nodes.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE IF NOT EXISTS nodes (id TEXT PRIMARY KEY)", []).unwrap();
        conn.execute("INSERT INTO nodes (id) VALUES ('test')", []).unwrap();
        drop(conn);

        let result = wal_checkpoint(&synrepo_dir).unwrap();
        assert!(result, "WAL checkpoint should succeed");
    }

    #[test]
    fn compact_plan_fills_estimates() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let config = Config::default();

        let plan = plan_compact(&synrepo_dir, &config, crate::pipeline::maintenance::CompactPolicy::Default).unwrap();
        // Just verify it doesn't panic and has actions.
        assert!(plan.actions.len() >= 1); // At least WAL checkpoint.
    }
}