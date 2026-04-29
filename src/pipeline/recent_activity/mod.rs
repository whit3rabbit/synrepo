//! Bounded operational-history surface for `.synrepo/state/` and the overlay store.
//!
//! Exposes `read_recent_activity` — used by both `synrepo status --recent` and
//! the `synrepo_recent_activity` MCP tool — which fans out to per-kind readers
//! and merges results by timestamp.

use std::path::Path;

use anyhow::anyhow;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::config::Config;
use crate::pipeline::{
    git::{GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness},
    git_intelligence::GitHistoryIndex,
    repair::{repair_log_path, ResolutionLogEntry},
    watch::load_reconcile_state,
};

/// Maximum allowed limit for a single recent-activity query.
pub const MAX_ACTIVITY_LIMIT: usize = 200;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Which operational event stream to include in a recent-activity query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecentActivityKind {
    /// Most-recent reconcile outcome (at most one entry).
    Reconcile,
    /// Entries from `repair-log.jsonl`, most recent first.
    Repair,
    /// Events from the `cross_link_audit` table, most recent first.
    CrossLink,
    /// Commentary rows from the overlay store, most recent first.
    OverlayRefresh,
    /// Top-N churn-hot files derived from git history.
    Hotspot,
}

impl RecentActivityKind {
    /// Parse from a snake_case string. Returns `None` for unknown kinds.
    pub fn parse_kind(s: &str) -> Option<Self> {
        match s {
            "reconcile" => Some(Self::Reconcile),
            "repair" => Some(Self::Repair),
            "cross_link" => Some(Self::CrossLink),
            "overlay_refresh" => Some(Self::OverlayRefresh),
            "hotspot" => Some(Self::Hotspot),
            _ => None,
        }
    }

    fn all() -> Vec<Self> {
        vec![
            Self::Reconcile,
            Self::Repair,
            Self::CrossLink,
            Self::OverlayRefresh,
            Self::Hotspot,
        ]
    }
}

/// A single operational activity event.
#[derive(Clone, Debug, Serialize)]
pub struct ActivityEntry {
    /// Kind label (snake_case).
    pub kind: String,
    /// RFC 3339 UTC timestamp. Empty for hotspot entries (point-in-time).
    pub timestamp: String,
    /// Kind-specific payload.
    pub payload: Value,
}

/// Query parameters for recent-activity lookups.
pub struct RecentActivityQuery {
    /// Event kinds to include. `None` means all kinds.
    pub kinds: Option<Vec<RecentActivityKind>>,
    /// Maximum total entries to return (must be ≤ `MAX_ACTIVITY_LIMIT`).
    pub limit: usize,
    /// Exclude entries older than this RFC 3339 timestamp.
    pub since: Option<String>,
}

// ---------------------------------------------------------------------------
// Per-kind readers
// ---------------------------------------------------------------------------

/// Read the most recent reconcile outcome from `reconcile-state.json`.
///
/// Returns `None` when no reconcile-state file exists.
pub fn read_reconcile_event(synrepo_dir: &Path) -> Option<ActivityEntry> {
    let state = load_reconcile_state(synrepo_dir).ok()?;
    let payload = serde_json::json!({
        "outcome": state.last_outcome,
        "note": "single_entry",
        "files_discovered": state.files_discovered,
        "symbols_extracted": state.symbols_extracted,
        "triggering_events": state.triggering_events,
        "last_error": state.last_error,
    });
    Some(ActivityEntry {
        kind: "reconcile".to_string(),
        timestamp: state.last_reconcile_at,
        payload,
    })
}

/// Read recent repair events from `repair-log.jsonl`, most recent first.
///
/// Malformed JSONL lines are skipped, but a single warning is emitted per scan
/// containing the count so corruption is visible without flooding traces.
pub fn read_repair_events(
    synrepo_dir: &Path,
    limit: usize,
    since: Option<&str>,
) -> Vec<ActivityEntry> {
    let log_path = repair_log_path(synrepo_dir);
    if !log_path.exists() {
        return vec![];
    }
    let content = match std::fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut parse_errors: usize = 0;
    let mut entries: Vec<ActivityEntry> = Vec::new();
    for line in content.lines().rev().filter(|line| !line.trim().is_empty()) {
        if entries.len() >= limit {
            break;
        }
        let entry: ResolutionLogEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => {
                parse_errors += 1;
                continue;
            }
        };
        let timestamp = entry.synced_at.clone();
        if let Some(since) = since {
            if timestamp.as_str() < since {
                continue;
            }
        }
        let payload = match serde_json::to_value(&entry) {
            Ok(p) => p,
            Err(_) => {
                parse_errors += 1;
                continue;
            }
        };
        entries.push(ActivityEntry {
            kind: "repair".to_string(),
            timestamp,
            payload,
        });
    }
    if parse_errors > 0 {
        tracing::warn!(
            log_path = %log_path.display(),
            parse_errors,
            "repair-log contained malformed JSONL lines; skipped during recent_activity scan"
        );
    }
    entries
}

/// Read recent cross-link audit events from the overlay DB, most recent first.
pub fn read_cross_link_events(
    overlay_db_path: &Path,
    limit: usize,
    since: Option<&str>,
) -> Vec<ActivityEntry> {
    read_cross_link_events_inner(overlay_db_path, limit, since).unwrap_or_default()
}

fn read_cross_link_events_inner(
    overlay_db_path: &Path,
    limit: usize,
    since: Option<&str>,
) -> crate::Result<Vec<ActivityEntry>> {
    if !overlay_db_path.exists() {
        return Ok(vec![]);
    }
    let conn = Connection::open_with_flags(
        overlay_db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;

    // Empty string sorts before all RFC 3339 timestamps, so `>= ""` is always true.
    let since_str = since.unwrap_or("");
    let mut stmt = conn.prepare(
        "SELECT from_node, to_node, kind, event_kind, event_at \
         FROM cross_link_audit WHERE event_at >= ?1 \
         ORDER BY event_at DESC LIMIT ?2",
    )?;
    let rows: Vec<(String, String, String, String, String)> = stmt
        .query_map(rusqlite::params![since_str, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;

    Ok(rows
        .into_iter()
        .map(
            |(from_node, to_node, kind, event_kind, event_at)| ActivityEntry {
                kind: "cross_link".to_string(),
                timestamp: event_at,
                payload: serde_json::json!({
                    "from_node": from_node,
                    "to_node": to_node,
                    "kind": kind,
                    "event_kind": event_kind,
                }),
            },
        )
        .collect())
}

/// Read recent overlay commentary refresh events from the overlay DB, most recent first.
pub fn read_overlay_refresh_events(
    overlay_db_path: &Path,
    limit: usize,
    since: Option<&str>,
) -> Vec<ActivityEntry> {
    read_overlay_refresh_events_inner(overlay_db_path, limit, since).unwrap_or_default()
}

fn read_overlay_refresh_events_inner(
    overlay_db_path: &Path,
    limit: usize,
    since: Option<&str>,
) -> crate::Result<Vec<ActivityEntry>> {
    if !overlay_db_path.exists() {
        return Ok(vec![]);
    }
    let conn = Connection::open_with_flags(
        overlay_db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;

    // Empty string sorts before all RFC 3339 timestamps, so `>= ""` is always true.
    let since_str = since.unwrap_or("");
    let mut stmt = conn.prepare(
        "SELECT node_id, pass_id, generated_at FROM commentary \
         WHERE generated_at >= ?1 ORDER BY generated_at DESC LIMIT ?2",
    )?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map(rusqlite::params![since_str, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;

    Ok(rows
        .into_iter()
        .map(|(node_id, pass_id, generated_at)| ActivityEntry {
            kind: "overlay_refresh".to_string(),
            timestamp: generated_at,
            payload: serde_json::json!({
                "node_id": node_id,
                "pass_id": pass_id,
            }),
        })
        .collect())
}

/// Build git hotspot events from the in-memory `GitHistoryIndex`.
///
/// Returns a single entry with `state: "unavailable"` when git is absent.
/// Returns an empty list when git is available but no history was sampled.
pub fn read_hotspot_events(repo_root: &Path, config: &Config, limit: usize) -> Vec<ActivityEntry> {
    let context = GitIntelligenceContext::inspect(repo_root, config);

    // Bail early when the git repo itself is missing — don't try to walk history.
    if let GitIntelligenceReadiness::Degraded { ref reasons } = context.readiness() {
        if reasons.contains(&GitDegradedReason::RepositoryUnavailable) {
            return vec![ActivityEntry {
                kind: "hotspot".to_string(),
                timestamp: String::new(),
                payload: serde_json::json!({"state": "unavailable"}),
            }];
        }
    }

    let index = match GitHistoryIndex::build(&context, config.git_commit_depth as usize) {
        Ok(idx) => idx,
        Err(_) => {
            return vec![ActivityEntry {
                kind: "hotspot".to_string(),
                timestamp: String::new(),
                payload: serde_json::json!({"state": "unavailable"}),
            }];
        }
    };

    index
        .top_hotspots(limit)
        .into_iter()
        .map(|(path, touches)| ActivityEntry {
            kind: "hotspot".to_string(),
            timestamp: String::new(),
            payload: serde_json::json!({
                "path": path,
                "touches": touches,
                "source": "git_intelligence",
                "granularity": "file",
            }),
        })
        .collect()
}

/// Read and merge recent activity events across all requested kinds.
///
/// Returns `Err` when `query.limit > MAX_ACTIVITY_LIMIT`.
/// Results are sorted by timestamp descending; hotspot entries (no timestamp) sort last.
pub fn read_recent_activity(
    synrepo_dir: &Path,
    repo_root: &Path,
    config: &Config,
    query: RecentActivityQuery,
) -> crate::Result<Vec<ActivityEntry>> {
    if query.limit > MAX_ACTIVITY_LIMIT {
        return Err(anyhow!(
            "limit {} exceeds maximum allowed value of {}",
            query.limit,
            MAX_ACTIVITY_LIMIT
        )
        .into());
    }

    if let Some(since_value) = query.since.as_deref() {
        // The downstream readers do lexicographic string comparison against
        // stored RFC 3339 timestamps. Reject malformed `since` at the boundary
        // so callers don't get silent no-filter behavior from a typo.
        if OffsetDateTime::parse(since_value, &Rfc3339).is_err() {
            return Err(
                anyhow!("since value {since_value:?} is not a valid RFC 3339 timestamp").into(),
            );
        }
    }

    let kinds = query.kinds.unwrap_or_else(RecentActivityKind::all);
    let since = query.since.as_deref();
    let overlay_db = synrepo_dir.join("overlay").join("overlay.db");

    let mut entries: Vec<ActivityEntry> = Vec::new();

    for kind in &kinds {
        match kind {
            RecentActivityKind::Reconcile => {
                if let Some(entry) = read_reconcile_event(synrepo_dir) {
                    if since.is_none_or(|s| entry.timestamp.as_str() >= s) {
                        entries.push(entry);
                    }
                }
            }
            RecentActivityKind::Repair => {
                entries.extend(read_repair_events(synrepo_dir, query.limit, since));
            }
            RecentActivityKind::CrossLink => {
                entries.extend(read_cross_link_events(&overlay_db, query.limit, since));
            }
            RecentActivityKind::OverlayRefresh => {
                entries.extend(read_overlay_refresh_events(&overlay_db, query.limit, since));
            }
            RecentActivityKind::Hotspot => {
                entries.extend(read_hotspot_events(repo_root, config, query.limit));
            }
        }
    }

    // Sort by timestamp descending. Empty timestamps (hotspot) sort last.
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    entries.truncate(query.limit);

    Ok(entries)
}

#[cfg(test)]
mod tests;
