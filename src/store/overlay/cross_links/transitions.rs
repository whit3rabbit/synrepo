//! Candidate state transitions: reject, pending-promotion, promote, and
//! crash-recovery rollback. Each path records an audit event.

use rusqlite::{params, Connection, OptionalExtension};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::OverlayEdgeKind;

use super::super::cross_link_audit::{append_event, AuditEvent};
use super::super::with_write_transaction;
use super::codec::overlay_edge_kind_as_str;

/// Mark a candidate as rejected by a human reviewer.
pub(crate) fn mark_rejected(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    reviewer: &str,
) -> crate::Result<()> {
    let from_key = from.to_string();
    let to_key = to.to_string();
    let kind_str = overlay_edge_kind_as_str(kind);
    let prov: Option<(String, String, String)> = conn
        .query_row(
            "SELECT pass_id, model_identity, confidence_tier FROM cross_links
             WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
            params![from_key, to_key, kind_str],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((pass_id, model_identity, previous_tier)) = prov else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "mark_rejected: no candidate for ({from_key}, {to_key}, {kind_str})"
        )));
    };

    with_write_transaction(conn, |conn| {
        conn.execute(
            "UPDATE cross_links
             SET state = 'rejected', reviewer = ?1
             WHERE from_node = ?2 AND to_node = ?3 AND kind = ?4",
            params![reviewer, from_key, to_key, kind_str],
        )?;

        append_event(
            conn,
            &AuditEvent {
                from_node: &from_key,
                to_node: &to_key,
                kind: kind_str,
                event_kind: "rejected",
                reviewer: Some(reviewer),
                previous_tier: Some(&previous_tier),
                new_tier: Some(&previous_tier),
                reason: None,
                pass_id: &pass_id,
                model_identity: &model_identity,
            },
        )?;
        Ok(())
    })
}

/// Mark a candidate as pending promotion (atomicity bridge).
pub(crate) fn mark_pending(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    reviewer: &str,
) -> crate::Result<()> {
    let from_key = from.to_string();
    let to_key = to.to_string();
    let kind_str = overlay_edge_kind_as_str(kind);
    let prov: Option<(String, String, String)> = conn
        .query_row(
            "SELECT pass_id, model_identity, confidence_tier FROM cross_links
             WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
            params![from_key, to_key, kind_str],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((pass_id, model_identity, previous_tier)) = prov else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "mark_pending: no candidate for ({from_key}, {to_key}, {kind_str})"
        )));
    };

    with_write_transaction(conn, |conn| {
        conn.execute(
            "UPDATE cross_links
             SET state = 'pending_promotion', reviewer = ?1
             WHERE from_node = ?2 AND to_node = ?3 AND kind = ?4",
            params![reviewer, from_key, to_key, kind_str],
        )?;

        append_event(
            conn,
            &AuditEvent {
                from_node: &from_key,
                to_node: &to_key,
                kind: kind_str,
                event_kind: "pending_promotion",
                reviewer: Some(reviewer),
                previous_tier: Some(&previous_tier),
                new_tier: Some(&previous_tier),
                reason: None,
                pass_id: &pass_id,
                model_identity: &model_identity,
            },
        )?;
        Ok(())
    })
}

/// Mark a candidate as promoted into the graph. Records the reviewer and
/// back-references the resulting graph edge identifier.
pub(crate) fn mark_promoted(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    reviewer: &str,
    graph_edge_id: &str,
) -> crate::Result<()> {
    let from_key = from.to_string();
    let to_key = to.to_string();
    let kind_str = overlay_edge_kind_as_str(kind);
    let prov: Option<(String, String, String)> = conn
        .query_row(
            "SELECT pass_id, model_identity, confidence_tier FROM cross_links
             WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
            params![from_key, to_key, kind_str],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((pass_id, model_identity, previous_tier)) = prov else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "mark_promoted: no candidate for ({from_key}, {to_key}, {kind_str})"
        )));
    };

    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("invalid timestamp: {e}")))?;

    with_write_transaction(conn, |conn| {
        conn.execute(
            "UPDATE cross_links
             SET state = 'promoted', reviewer = ?1, promoted_at = ?2, graph_edge_id = ?3
             WHERE from_node = ?4 AND to_node = ?5 AND kind = ?6",
            params![reviewer, now, graph_edge_id, from_key, to_key, kind_str],
        )?;

        append_event(
            conn,
            &AuditEvent {
                from_node: &from_key,
                to_node: &to_key,
                kind: kind_str,
                event_kind: "promoted",
                reviewer: Some(reviewer),
                previous_tier: Some(&previous_tier),
                new_tier: Some(&previous_tier),
                reason: Some(graph_edge_id),
                pass_id: &pass_id,
                model_identity: &model_identity,
            },
        )?;
        Ok(())
    })
}

/// Reset a `pending_promotion` row back to `active` state. Used when crash
/// recovery determines that the graph edge was never written (Phase 2 did not
/// complete). The candidate can then be re-accepted.
pub(crate) fn reset_pending_to_active(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
) -> crate::Result<()> {
    let from_key = from.to_string();
    let to_key = to.to_string();
    let kind_str = overlay_edge_kind_as_str(kind);
    let prov: Option<(String, String, String)> = conn
        .query_row(
            "SELECT pass_id, model_identity, confidence_tier FROM cross_links
             WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
            params![from_key, to_key, kind_str],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((pass_id, model_identity, previous_tier)) = prov else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "reset_pending_to_active: no candidate for ({from_key}, {to_key}, {kind_str})"
        )));
    };

    with_write_transaction(conn, |conn| {
        conn.execute(
            "UPDATE cross_links
             SET state = 'active', reviewer = NULL, promoted_at = NULL, graph_edge_id = NULL
             WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
            params![from_key, to_key, kind_str],
        )?;

        append_event(
            conn,
            &AuditEvent {
                from_node: &from_key,
                to_node: &to_key,
                kind: kind_str,
                event_kind: "promotion_rolled_back",
                reviewer: None,
                previous_tier: Some(&previous_tier),
                new_tier: Some(&previous_tier),
                reason: Some("crash_recovery_no_edge"),
                pass_id: &pass_id,
                model_identity: &model_identity,
            },
        )?;
        Ok(())
    })
}
