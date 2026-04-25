//! Read-only queries over the `cross_links` table.

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::ids::NodeId;
use crate::overlay::{OverlayEdgeKind, OverlayLink};

use super::super::sqlite_values::{row_usize, usize_to_i64};
use super::codec::{overlay_edge_kind_as_str, row_to_overlay_link};
use super::types::{CrossLinkHashRow, PendingPromotionRow};

const SELECT_OVERLAY_LINK_COLUMNS: &str = "from_node, to_node, kind, epistemic,
                source_spans_json, target_spans_json,
                from_content_hash, to_content_hash,
                confidence_score, confidence_tier, rationale,
                pass_id, model_identity, generated_at";

/// Retrieve every candidate touching `node` as either endpoint, active or not.
/// Results are ordered by confidence score descending, then by generated_at.
pub(crate) fn candidates_for_node(
    conn: &Connection,
    node: NodeId,
) -> crate::Result<Vec<OverlayLink>> {
    let key = node.to_string();
    let sql = format!(
        "SELECT {SELECT_OVERLAY_LINK_COLUMNS}
         FROM cross_links
         WHERE from_node = ?1 OR to_node = ?1
         ORDER BY confidence_score DESC, generated_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![key], row_to_overlay_link)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    rows.into_iter().collect::<crate::Result<Vec<_>>>()
}

/// Retrieve all active candidates, optionally filtered by tier.
pub(crate) fn all_candidates(
    conn: &Connection,
    tier: Option<&str>,
) -> crate::Result<Vec<OverlayLink>> {
    let sql = format!(
        "SELECT {SELECT_OVERLAY_LINK_COLUMNS}
         FROM cross_links
         WHERE state = 'active' AND (?1 IS NULL OR confidence_tier = ?1)
         ORDER BY confidence_score DESC, generated_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;

    let mapped = stmt
        .query_map(params![tier], row_to_overlay_link)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    mapped.into_iter().collect()
}

/// Retrieve active candidates filtered by tier, with SQL-side limit applied
/// before materialization. Use this instead of `all_candidates` + `truncate`
/// to avoid loading the full candidate set when only a bounded page is needed.
pub(crate) fn candidates_limited(
    conn: &Connection,
    tier: Option<&str>,
    limit: usize,
) -> crate::Result<Vec<OverlayLink>> {
    let sql = format!(
        "SELECT {SELECT_OVERLAY_LINK_COLUMNS}
         FROM cross_links
         WHERE state = 'active' AND (?1 IS NULL OR confidence_tier = ?1)
         ORDER BY confidence_score DESC, generated_at DESC
         LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let limit = usize_to_i64(limit, "candidate limit")?;

    let mapped = stmt
        .query_map(params![tier, limit], row_to_overlay_link)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    mapped.into_iter().collect()
}

/// Look up a single candidate by its `(from, to, kind)` triple. Returns
/// `None` when no row matches. Used by the revalidation handler before
/// calling the fuzzy-LCS verifier.
pub(crate) fn candidate_by_endpoints(
    conn: &Connection,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
) -> crate::Result<Option<OverlayLink>> {
    let from_key = from.to_string();
    let to_key = to.to_string();
    let kind_str = overlay_edge_kind_as_str(kind);
    let sql = format!(
        "SELECT {SELECT_OVERLAY_LINK_COLUMNS}
         FROM cross_links
         WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let row = stmt
        .query_row(params![from_key, to_key, kind_str], row_to_overlay_link)
        .optional()?;
    match row {
        Some(result) => result.map(Some),
        None => Ok(None),
    }
}

/// Count cross-link rows currently stored.
pub(crate) fn count(conn: &Connection) -> crate::Result<usize> {
    Ok(
        conn.query_row("SELECT COUNT(*) FROM cross_links", [], |row| {
            row_usize(row, 0)
        })?,
    )
}

/// Return every candidate's endpoint keys and stored hashes. Used by the
/// repair loop to classify freshness without reconstructing full spans.
pub(crate) fn endpoint_hashes(conn: &Connection) -> crate::Result<Vec<CrossLinkHashRow>> {
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

/// Return all cross-link rows stuck in `pending_promotion` state.
/// Used by the repair loop to resolve incomplete promotions after crashes.
pub(crate) fn pending_promotion_rows(conn: &Connection) -> crate::Result<Vec<PendingPromotionRow>> {
    let mut stmt = conn.prepare(
        "SELECT from_node, to_node, kind, reviewer
         FROM cross_links
         WHERE state = 'pending_promotion'",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PendingPromotionRow {
                from_node: row.get::<_, String>(0)?,
                to_node: row.get::<_, String>(1)?,
                kind: row.get::<_, String>(2)?,
                reviewer: row.get::<_, Option<String>>(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
