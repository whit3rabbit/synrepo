//! Immutable audit trail for cross-link candidates.
//!
//! Rows are append-only. There is no UPDATE or DELETE accessor — deletion of
//! a candidate from `cross_links` leaves the audit history intact, so a
//! reviewer can reconstruct the full lifecycle of any candidate that ever
//! existed.

use rusqlite::{params, Connection};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use super::SqliteOverlayStore;

/// Event recorded in the `cross_link_audit` table.
///
/// All fields are borrowed so callers can assemble an event without
/// intermediate allocations. The database copy is taken on insert.
#[derive(Clone, Debug)]
pub(super) struct AuditEvent<'a> {
    /// Source endpoint node ID (display form).
    pub from_node: &'a str,
    /// Target endpoint node ID (display form).
    pub to_node: &'a str,
    /// `OverlayEdgeKind` snake_case identifier.
    pub kind: &'a str,
    /// Lifecycle event label, e.g. `generated`, `promoted`, `pruned`.
    pub event_kind: &'a str,
    /// Reviewer identity (for `rejected`/`promoted` events).
    pub reviewer: Option<&'a str>,
    /// Confidence tier before the event, when applicable.
    pub previous_tier: Option<&'a str>,
    /// Confidence tier after the event, when applicable.
    pub new_tier: Option<&'a str>,
    /// Free-form reason string, e.g. `source_deleted`, `spans_reverified`.
    pub reason: Option<&'a str>,
    /// Generation pass identifier, copied from the candidate.
    pub pass_id: &'a str,
    /// Model identity, copied from the candidate.
    pub model_identity: &'a str,
}

/// Append an immutable audit row.
pub(super) fn append_event(conn: &Connection, event: &AuditEvent<'_>) -> crate::Result<()> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("invalid timestamp: {e}")))?;

    conn.execute(
        "INSERT INTO cross_link_audit
            (from_node, to_node, kind, event_kind, reviewer,
             previous_tier, new_tier, reason,
             pass_id, model_identity, event_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            event.from_node,
            event.to_node,
            event.kind,
            event.event_kind,
            event.reviewer,
            event.previous_tier,
            event.new_tier,
            event.reason,
            event.pass_id,
            event.model_identity,
            now,
        ],
    )?;
    Ok(())
}

/// Retrieve the ordered audit trail for a candidate triple. Returns events
/// in insertion order.
pub(super) fn events_for_candidate(
    conn: &Connection,
    from_node: &str,
    to_node: &str,
    kind: &str,
) -> crate::Result<Vec<AuditRow>> {
    let mut stmt = conn.prepare(
        "SELECT event_kind, reviewer, previous_tier, new_tier, reason,
                pass_id, model_identity, event_at
         FROM cross_link_audit
         WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3
         ORDER BY id ASC",
    )?;
    let rows = stmt
        .query_map(params![from_node, to_node, kind], |row| {
            Ok(AuditRow {
                event_kind: row.get(0)?,
                reviewer: row.get(1)?,
                previous_tier: row.get(2)?,
                new_tier: row.get(3)?,
                reason: row.get(4)?,
                pass_id: row.get(5)?,
                model_identity: row.get(6)?,
                event_at: row.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Materialized audit row returned to callers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRow {
    /// Lifecycle event label.
    pub event_kind: String,
    /// Reviewer identity recorded with the event, if any.
    pub reviewer: Option<String>,
    /// Confidence tier before the event, when applicable.
    pub previous_tier: Option<String>,
    /// Confidence tier after the event, when applicable.
    pub new_tier: Option<String>,
    /// Free-form reason string.
    pub reason: Option<String>,
    /// Generation pass identifier.
    pub pass_id: String,
    /// Model identity.
    pub model_identity: String,
    /// Timestamp (RFC 3339 UTC) when the event was written.
    pub event_at: String,
}

impl SqliteOverlayStore {
    /// Return the ordered audit trail for a stored candidate.
    pub fn cross_link_audit_events(
        &self,
        from_node: &str,
        to_node: &str,
        kind: &str,
    ) -> crate::Result<Vec<AuditRow>> {
        let conn = self.conn.lock();
        events_for_candidate(&conn, from_node, to_node, kind)
    }

    /// Return the total count of audit rows. Used by tests.
    pub fn cross_link_audit_count(&self) -> crate::Result<usize> {
        let conn = self.conn.lock();
        Ok(
            conn.query_row("SELECT COUNT(*) FROM cross_link_audit", [], |row| {
                row.get::<_, usize>(0)
            })?,
        )
    }
}
