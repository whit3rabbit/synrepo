//! Retention and compaction helpers for commentary-owned overlay surfaces.

use rusqlite::params;

use crate::pipeline::maintenance::{retention_cutoff, CompactPolicy, CompactStats};

use super::super::{sqlite_values::row_usize, SqliteOverlayStore};

pub(super) fn compactable_commentary_stats(
    store: &SqliteOverlayStore,
    policy: &CompactPolicy,
) -> crate::Result<CompactStats> {
    let cutoff_str = retention_cutoff(policy.commentary_retention_days())?;

    let conn = store.conn.lock();
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM commentary WHERE generated_at < ?1",
        params![cutoff_str],
        |row| row_usize(row, 0),
    )?;

    Ok(CompactStats {
        compactable_commentary: count,
        compactable_cross_links: 0,
        repair_log_entries_beyond_window: 0,
        last_compaction_timestamp: None,
    })
}

pub(super) fn compact_commentary(
    store: &mut SqliteOverlayStore,
    policy: &CompactPolicy,
) -> crate::Result<usize> {
    let cutoff_str = retention_cutoff(policy.commentary_retention_days())?;

    let conn = store.conn.lock();
    let deleted = conn.execute(
        "DELETE FROM commentary WHERE generated_at < ?1",
        params![cutoff_str],
    )?;

    Ok(deleted)
}

pub(super) fn compactable_cross_link_stats(
    store: &SqliteOverlayStore,
    policy: &CompactPolicy,
) -> crate::Result<CompactStats> {
    let cutoff_str = retention_cutoff(policy.audit_retention_days())?;

    let conn = store.conn.lock();
    // `cross_link_audit` records lifecycle events; the column that captures
    // terminal state is `event_kind` ('promoted' / 'rejected'), not `state`
    // (which lives on `cross_links`, not on the audit table; see docs/SCHEMA.md).
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM cross_link_audit \
         WHERE event_kind IN ('promoted', 'rejected') AND event_at < ?1",
        params![cutoff_str],
        |row| row_usize(row, 0),
    )?;

    Ok(CompactStats {
        compactable_commentary: 0,
        compactable_cross_links: count,
        repair_log_entries_beyond_window: 0,
        last_compaction_timestamp: None,
    })
}

pub(super) fn compact_cross_links(
    store: &mut SqliteOverlayStore,
    policy: &CompactPolicy,
) -> crate::Result<usize> {
    let cutoff_str = retention_cutoff(policy.audit_retention_days())?;

    let conn = store.conn.lock();
    // See `compactable_cross_link_stats` for the `event_kind` vs `state`
    // column note.
    let deleted = conn.execute(
        "DELETE FROM cross_link_audit \
         WHERE event_kind IN ('promoted', 'rejected') AND event_at < ?1",
        params![cutoff_str],
    )?;

    Ok(deleted)
}

pub(super) fn cross_link_audit_count(store: &SqliteOverlayStore) -> crate::Result<usize> {
    let conn = store.conn.lock();
    Ok(
        conn.query_row("SELECT COUNT(*) FROM cross_link_audit", [], |row| {
            row_usize(row, 0)
        })?,
    )
}
