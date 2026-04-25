//! Mutations that create or refresh candidate rows: insert/upsert, tier
//! updates, hash refreshes after revalidation, and orphan pruning.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use time::format_description::well_known::Rfc3339;

use crate::core::ids::NodeId;
use crate::overlay::{CitedSpan, ConfidenceTier, OverlayEdgeKind, OverlayLink};

use super::super::cross_link_audit::{append_event, AuditEvent};
use super::codec::{anyhow_err, overlay_edge_kind_as_str, overlay_epistemic_as_str, validate_link};

/// Insert or refresh a cross-link candidate.
///
/// Keyed on `(from_node, to_node, kind)` — inserting a second candidate for
/// the same triple upserts and appends an audit row with event
/// `regenerated`. A brand-new triple appends event `generated`.
pub(crate) fn insert_candidate(conn: &Connection, link: &OverlayLink) -> crate::Result<()> {
    validate_link(link)?;

    let from_key = link.from.to_string();
    let to_key = link.to.to_string();
    let kind = overlay_edge_kind_as_str(link.kind);
    let epistemic = overlay_epistemic_as_str(link.epistemic);
    let tier = link.confidence_tier.as_str();
    let source_spans = serde_json::to_string(&link.source_spans).map_err(anyhow_err)?;
    let target_spans = serde_json::to_string(&link.target_spans).map_err(anyhow_err)?;
    let generated_at = link
        .provenance
        .generated_at
        .format(&Rfc3339)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("invalid timestamp: {e}")))?;

    let existing_tier: Option<String> = conn
        .query_row(
            "SELECT confidence_tier FROM cross_links
             WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
            params![from_key, to_key, kind],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    conn.execute(
        "INSERT INTO cross_links
            (from_node, to_node, kind, epistemic,
             source_spans_json, target_spans_json,
             from_content_hash, to_content_hash,
             confidence_score, confidence_tier, rationale,
             pass_id, model_identity, generated_at, state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'active')
         ON CONFLICT(from_node, to_node, kind) DO UPDATE SET
             epistemic = excluded.epistemic,
             source_spans_json = excluded.source_spans_json,
             target_spans_json = excluded.target_spans_json,
             from_content_hash = excluded.from_content_hash,
             to_content_hash = excluded.to_content_hash,
             confidence_score = excluded.confidence_score,
             confidence_tier = excluded.confidence_tier,
             rationale = excluded.rationale,
             pass_id = excluded.pass_id,
             model_identity = excluded.model_identity,
             generated_at = excluded.generated_at,
             state = 'active',
             reviewer = NULL,
             promoted_at = NULL,
             graph_edge_id = NULL",
        params![
            from_key,
            to_key,
            kind,
            epistemic,
            source_spans,
            target_spans,
            link.from_content_hash,
            link.to_content_hash,
            link.confidence_score,
            tier,
            link.rationale,
            link.provenance.pass_id,
            link.provenance.model_identity,
            generated_at,
        ],
    )?;

    let event_kind = if existing_tier.is_some() {
        "regenerated"
    } else {
        "generated"
    };
    append_event(
        conn,
        &AuditEvent {
            from_node: &from_key,
            to_node: &to_key,
            kind,
            event_kind,
            reviewer: None,
            previous_tier: existing_tier.as_deref(),
            new_tier: Some(tier),
            reason: None,
            pass_id: &link.provenance.pass_id,
            model_identity: &link.provenance.model_identity,
        },
    )?;

    Ok(())
}

/// Delete every candidate whose either endpoint is absent from `live`.
/// Appends one audit row per deletion with reason `source_deleted`.
pub(crate) fn prune_orphans(conn: &Connection, live: &HashSet<String>) -> crate::Result<usize> {
    // Materialize a plain snapshot first so we can both decide-to-delete and
    // record audit rows without holding a prepared statement across writes.
    let mut stmt = conn.prepare(
        "SELECT from_node, to_node, kind, confidence_tier, pass_id, model_identity
         FROM cross_links",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut removed = 0usize;
    for (from_node, to_node, kind, tier, pass_id, model_identity) in rows {
        if !live.contains(&from_node) || !live.contains(&to_node) {
            conn.execute(
                "DELETE FROM cross_links
                 WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
                params![from_node, to_node, kind],
            )?;
            append_event(
                conn,
                &AuditEvent {
                    from_node: &from_node,
                    to_node: &to_node,
                    kind: &kind,
                    event_kind: "pruned",
                    reviewer: None,
                    previous_tier: Some(&tier),
                    new_tier: None,
                    reason: Some("source_deleted"),
                    pass_id: &pass_id,
                    model_identity: &model_identity,
                },
            )?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Update the confidence tier of a stored candidate after revalidation.
/// Appends an audit row with event kind `tier_changed`.
pub(crate) fn update_tier(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    new_tier: ConfidenceTier,
    new_score: f32,
    reason: &str,
) -> crate::Result<()> {
    let from_key = from.to_string();
    let to_key = to.to_string();
    let kind_str = overlay_edge_kind_as_str(kind);
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT confidence_tier, pass_id, model_identity FROM cross_links
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

    let Some((previous_tier, pass_id, model_identity)) = row else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "update_tier: no candidate for ({from_key}, {to_key}, {kind_str})"
        )));
    };
    let previous_tier = Some(previous_tier);

    conn.execute(
        "UPDATE cross_links
         SET confidence_tier = ?1, confidence_score = ?2
         WHERE from_node = ?3 AND to_node = ?4 AND kind = ?5",
        params![new_tier.as_str(), new_score, from_key, to_key, kind_str],
    )?;

    append_event(
        conn,
        &AuditEvent {
            from_node: &from_key,
            to_node: &to_key,
            kind: kind_str,
            event_kind: "tier_changed",
            reviewer: None,
            previous_tier: previous_tier.as_deref(),
            new_tier: Some(new_tier.as_str()),
            reason: Some(reason),
            pass_id: &pass_id,
            model_identity: &model_identity,
        },
    )?;
    Ok(())
}

/// Refresh a candidate's stored endpoint hashes and verified spans after a
/// successful fuzzy-LCS revalidation. State, tier, reviewer, and promotion
/// columns are preserved. Appends an audit row with event kind `revalidated`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refresh_hashes(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    new_from_hash: &str,
    new_to_hash: &str,
    new_source_spans: &[CitedSpan],
    new_target_spans: &[CitedSpan],
) -> crate::Result<()> {
    if new_source_spans.is_empty() || new_target_spans.is_empty() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "refresh_hashes: verified spans must be non-empty for both endpoints"
        )));
    }

    let from_key = from.to_string();
    let to_key = to.to_string();
    let kind_str = overlay_edge_kind_as_str(kind);
    let source_spans_json = serde_json::to_string(new_source_spans).map_err(anyhow_err)?;
    let target_spans_json = serde_json::to_string(new_target_spans).map_err(anyhow_err)?;

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
    let Some((pass_id, model_identity, tier)) = prov else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "refresh_hashes: no candidate for ({from_key}, {to_key}, {kind_str})"
        )));
    };

    conn.execute(
        "UPDATE cross_links
         SET from_content_hash = ?1, to_content_hash = ?2,
             source_spans_json = ?3, target_spans_json = ?4
         WHERE from_node = ?5 AND to_node = ?6 AND kind = ?7",
        params![
            new_from_hash,
            new_to_hash,
            source_spans_json,
            target_spans_json,
            from_key,
            to_key,
            kind_str
        ],
    )?;

    append_event(
        conn,
        &AuditEvent {
            from_node: &from_key,
            to_node: &to_key,
            kind: kind_str,
            event_kind: "revalidated",
            reviewer: None,
            previous_tier: Some(&tier),
            new_tier: Some(&tier),
            reason: Some("spans_reverified"),
            pass_id: &pass_id,
            model_identity: &model_identity,
        },
    )?;
    Ok(())
}
