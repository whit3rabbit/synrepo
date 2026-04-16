//! Cross-link candidate persistence for `SqliteOverlayStore`.
//!
//! This module owns the `cross_links` table. All mutation paths go through
//! functions here, which also append audit rows for every lifecycle event.
//! Read paths reconstruct `OverlayLink` values from stored columns.

use std::collections::HashSet;
use std::str::FromStr;

use rusqlite::{params, Connection, OptionalExtension};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, CrossLinkState, OverlayEdgeKind,
    OverlayEpistemic, OverlayLink,
};

use super::cross_link_audit::{append_event, AuditEvent};
use super::SqliteOverlayStore;

/// Insert or refresh a cross-link candidate.
///
/// Keyed on `(from_node, to_node, kind)` — inserting a second candidate for
/// the same triple upserts and appends an audit row with event
/// `regenerated`. A brand-new triple appends event `generated`.
pub(super) fn insert_candidate(conn: &Connection, link: &OverlayLink) -> crate::Result<()> {
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

/// Retrieve every candidate touching `node` as either endpoint, active or not.
/// Results are ordered by confidence score descending, then by generated_at.
pub(super) fn candidates_for_node(
    conn: &Connection,
    node: NodeId,
) -> crate::Result<Vec<OverlayLink>> {
    let key = node.to_string();
    let mut stmt = conn.prepare(
        "SELECT from_node, to_node, kind, epistemic,
                source_spans_json, target_spans_json,
                from_content_hash, to_content_hash,
                confidence_score, confidence_tier, rationale,
                pass_id, model_identity, generated_at
         FROM cross_links
         WHERE from_node = ?1 OR to_node = ?1
         ORDER BY confidence_score DESC, generated_at DESC",
    )?;
    let rows = stmt
        .query_map(params![key], row_to_overlay_link)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    rows.into_iter().collect::<crate::Result<Vec<_>>>()
}

/// Retrieve all active candidates, optionally filtered by tier.
pub(super) fn all_candidates(
    conn: &Connection,
    tier: Option<&str>,
) -> crate::Result<Vec<OverlayLink>> {
    let mut stmt = conn.prepare(
        "SELECT from_node, to_node, kind, epistemic,
                source_spans_json, target_spans_json,
                from_content_hash, to_content_hash,
                confidence_score, confidence_tier, rationale,
                pass_id, model_identity, generated_at
         FROM cross_links
         WHERE state = 'active' AND (?1 IS NULL OR confidence_tier = ?1)
         ORDER BY confidence_score DESC, generated_at DESC",
    )?;

    let mapped = stmt
        .query_map(params![tier], row_to_overlay_link)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    mapped.into_iter().collect()
}

/// Count cross-link rows currently stored.
pub(super) fn count(conn: &Connection) -> crate::Result<usize> {
    Ok(
        conn.query_row("SELECT COUNT(*) FROM cross_links", [], |row| {
            row.get::<_, usize>(0)
        })?,
    )
}

/// Return every candidate's endpoint keys and stored hashes. Used by the
/// repair loop to classify freshness without reconstructing full spans.
pub(super) fn endpoint_hashes(conn: &Connection) -> crate::Result<Vec<CrossLinkHashRow>> {
    let mut stmt = conn.prepare(
        "SELECT from_node, to_node, kind, from_content_hash, to_content_hash, confidence_tier, state
         FROM cross_links",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CrossLinkHashRow {
                from_node: row.get::<_, String>(0)?,
                to_node: row.get::<_, String>(1)?,
                kind: row.get::<_, String>(2)?,
                from_content_hash: row.get::<_, String>(3)?,
                to_content_hash: row.get::<_, String>(4)?,
                confidence_tier: row.get::<_, String>(5)?,
                state: row.get::<_, String>(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Delete every candidate whose either endpoint is absent from `live`.
/// Appends one audit row per deletion with reason `source_deleted`.
pub(super) fn prune_orphans(conn: &Connection, live: &HashSet<String>) -> crate::Result<usize> {
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
pub(super) fn update_tier(
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

/// Refresh a candidate's stored endpoint hashes after a successful
/// revalidation. Appends an audit row with event kind `revalidated`.
pub(super) fn refresh_hashes(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    new_from_hash: &str,
    new_to_hash: &str,
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
    let Some((pass_id, model_identity, tier)) = prov else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "refresh_hashes: no candidate for ({from_key}, {to_key}, {kind_str})"
        )));
    };

    conn.execute(
        "UPDATE cross_links
         SET from_content_hash = ?1, to_content_hash = ?2
         WHERE from_node = ?3 AND to_node = ?4 AND kind = ?5",
        params![new_from_hash, new_to_hash, from_key, to_key, kind_str],
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

/// Mark a candidate as rejected by a human reviewer.
pub(super) fn mark_rejected(
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
}

/// Mark a candidate as pending promotion (atomicity bridge).
pub fn mark_pending(
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
}

/// Mark a candidate as promoted into the graph. Records the reviewer and
/// back-references the resulting graph edge identifier.
pub fn mark_promoted(
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
}

// ---------- shared helpers ----------

fn row_to_overlay_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::Result<OverlayLink>> {
    Ok((|| -> crate::Result<OverlayLink> {
        let from_node: String = row.get(0)?;
        let to_node: String = row.get(1)?;
        let kind: String = row.get(2)?;
        let epistemic: String = row.get(3)?;
        let source_spans_json: String = row.get(4)?;
        let target_spans_json: String = row.get(5)?;
        let from_content_hash: String = row.get(6)?;
        let to_content_hash: String = row.get(7)?;
        let confidence_score: f32 = row.get(8)?;
        let confidence_tier: String = row.get(9)?;
        let rationale: Option<String> = row.get(10)?;
        let pass_id: String = row.get(11)?;
        let model_identity: String = row.get(12)?;
        let generated_at: String = row.get(13)?;

        let from = NodeId::from_str(&from_node)
            .map_err(|e| anyhow::anyhow!("stored from_node invalid: {e}"))?;
        let to = NodeId::from_str(&to_node)
            .map_err(|e| anyhow::anyhow!("stored to_node invalid: {e}"))?;
        let kind = parse_overlay_edge_kind(&kind)?;
        let epistemic = parse_overlay_epistemic(&epistemic)?;
        let tier = parse_confidence_tier(&confidence_tier)?;
        let source_spans: Vec<CitedSpan> =
            serde_json::from_str(&source_spans_json).map_err(anyhow_err)?;
        let target_spans: Vec<CitedSpan> =
            serde_json::from_str(&target_spans_json).map_err(anyhow_err)?;
        let generated_at = OffsetDateTime::parse(&generated_at, &Rfc3339)
            .map_err(|e| anyhow::anyhow!("invalid generated_at: {e}"))?;

        Ok(OverlayLink {
            from,
            to,
            kind,
            epistemic,
            source_spans,
            target_spans,
            from_content_hash,
            to_content_hash,
            confidence_score,
            confidence_tier: tier,
            rationale,
            provenance: CrossLinkProvenance {
                pass_id,
                model_identity,
                generated_at,
            },
        })
    })())
}

fn validate_link(link: &OverlayLink) -> crate::Result<()> {
    if !link.has_complete_provenance() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate is missing required provenance fields"
        )));
    }
    if link.source_spans.is_empty() || link.target_spans.is_empty() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate must carry at least one source and one target span"
        )));
    }
    if link.from_content_hash.is_empty() || link.to_content_hash.is_empty() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate must carry both endpoint content hashes"
        )));
    }
    if link.from == link.to {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate from/to must be distinct nodes"
        )));
    }
    Ok(())
}

pub(super) fn overlay_edge_kind_as_str(k: OverlayEdgeKind) -> &'static str {
    match k {
        OverlayEdgeKind::References => "references",
        OverlayEdgeKind::Governs => "governs",
        OverlayEdgeKind::DerivedFrom => "derived_from",
        OverlayEdgeKind::Mentions => "mentions",
    }
}

pub(super) fn parse_overlay_edge_kind(s: &str) -> crate::Result<OverlayEdgeKind> {
    match s {
        "references" => Ok(OverlayEdgeKind::References),
        "governs" => Ok(OverlayEdgeKind::Governs),
        "derived_from" => Ok(OverlayEdgeKind::DerivedFrom),
        "mentions" => Ok(OverlayEdgeKind::Mentions),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid overlay edge kind: {other}"
        ))),
    }
}

fn overlay_epistemic_as_str(e: OverlayEpistemic) -> &'static str {
    match e {
        OverlayEpistemic::MachineAuthoredHighConf => "machine_authored_high_conf",
        OverlayEpistemic::MachineAuthoredLowConf => "machine_authored_low_conf",
    }
}

fn parse_overlay_epistemic(s: &str) -> crate::Result<OverlayEpistemic> {
    match s {
        "machine_authored_high_conf" => Ok(OverlayEpistemic::MachineAuthoredHighConf),
        "machine_authored_low_conf" => Ok(OverlayEpistemic::MachineAuthoredLowConf),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid overlay epistemic: {other}"
        ))),
    }
}

pub(super) fn parse_confidence_tier(s: &str) -> crate::Result<ConfidenceTier> {
    match s {
        "high" => Ok(ConfidenceTier::High),
        "review_queue" => Ok(ConfidenceTier::ReviewQueue),
        "below_threshold" => Ok(ConfidenceTier::BelowThreshold),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid confidence tier: {other}"
        ))),
    }
}

#[allow(dead_code)]
pub(super) fn parse_state(s: &str) -> crate::Result<CrossLinkState> {
    match s {
        "active" => Ok(CrossLinkState::Active),
        "promoted" => Ok(CrossLinkState::Promoted),
        "rejected" => Ok(CrossLinkState::Rejected),
        "pending_promotion" => Ok(CrossLinkState::PendingPromotion),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid cross-link state: {other}"
        ))),
    }
}

fn anyhow_err<E: std::fmt::Display>(e: E) -> crate::Error {
    crate::Error::Other(anyhow::anyhow!("{e}"))
}

/// Row-level snapshot used by the repair loop. Strings are the stored
/// serialized forms — node IDs in display format and the tier/state enums'
/// snake_case identifiers.
#[derive(Clone, Debug)]
pub struct CrossLinkHashRow {
    /// Source endpoint node ID (display form).
    pub from_node: String,
    /// Target endpoint node ID (display form).
    pub to_node: String,
    /// `OverlayEdgeKind` snake_case identifier.
    pub kind: String,
    /// Source endpoint content hash as of generation time.
    pub from_content_hash: String,
    /// Target endpoint content hash as of generation time.
    pub to_content_hash: String,
    /// `ConfidenceTier` snake_case identifier.
    pub confidence_tier: String,
    /// `CrossLinkState` snake_case identifier.
    pub state: String,
}

impl SqliteOverlayStore {
    /// Return the number of cross-link rows currently stored.
    pub fn cross_link_count(&self) -> crate::Result<usize> {
        let conn = self.conn.lock();
        count(&conn)
    }

    /// Return every candidate's endpoint keys and stored hashes.
    pub fn cross_link_hashes(&self) -> crate::Result<Vec<CrossLinkHashRow>> {
        let conn = self.conn.lock();
        endpoint_hashes(&conn)
    }

    /// Retrieve all active candidates, optionally filtered by tier.
    pub fn all_candidates(&self, tier: Option<&str>) -> crate::Result<Vec<OverlayLink>> {
        let conn = self.conn.lock();
        all_candidates(&conn, tier)
    }

    /// Refresh stored endpoint hashes for a candidate after a successful
    /// fuzzy-LCS revalidation.
    pub fn refresh_candidate_hashes(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        new_from_hash: &str,
        new_to_hash: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        refresh_hashes(&conn, from, to, kind, new_from_hash, new_to_hash)
    }

    /// Update a candidate's confidence tier. Use after revalidation fails.
    pub fn update_candidate_tier(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        new_tier: ConfidenceTier,
        new_score: f32,
        reason: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        update_tier(&conn, from, to, kind, new_tier, new_score, reason)
    }

    /// Mark a candidate as rejected by a human reviewer.
    pub fn mark_candidate_rejected(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        mark_rejected(&conn, from, to, kind, reviewer)
    }

    /// Mark a candidate as pending promotion (atomicity bridge).
    pub fn mark_candidate_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        mark_pending(&conn, from, to, kind, reviewer)
    }

    /// Mark a candidate as promoted into the graph. The graph-side write is
    pub fn mark_candidate_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
        graph_edge_id: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        mark_promoted(&conn, from, to, kind, reviewer, graph_edge_id)
    }
}
