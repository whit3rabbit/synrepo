use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use super::ActivityEntry;

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
