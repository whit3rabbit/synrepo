//! Commentary CRUD and freshness derivation for `SqliteOverlayStore`.

use std::collections::HashSet;
use std::str::FromStr;

use rusqlite::{params, OptionalExtension};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{CommentaryEntry, CommentaryProvenance, FreshnessState, OverlayStore};

use super::SqliteOverlayStore;

/// Derive the freshness state of a commentary entry relative to the current
/// content hash of the annotated node's file.
///
/// Returns `Invalid` if any required provenance field is empty (defensive;
/// `insert_commentary` rejects these on the way in). Returns `Fresh` on a hash
/// match, `Stale` on mismatch.
pub fn derive_freshness(entry: &CommentaryEntry, current_content_hash: &str) -> FreshnessState {
    if !has_complete_provenance(&entry.provenance) {
        return FreshnessState::Invalid;
    }
    if entry.provenance.source_content_hash == current_content_hash {
        FreshnessState::Fresh
    } else {
        FreshnessState::Stale
    }
}

fn has_complete_provenance(prov: &CommentaryProvenance) -> bool {
    !prov.source_content_hash.is_empty()
        && !prov.pass_id.is_empty()
        && !prov.model_identity.is_empty()
}

impl OverlayStore for SqliteOverlayStore {
    fn insert_link(&mut self, link: crate::overlay::OverlayLink) -> crate::Result<()> {
        let conn = self.conn.lock();
        super::cross_links::insert_candidate(&conn, &link)
    }

    fn links_for(&self, node: NodeId) -> crate::Result<Vec<crate::overlay::OverlayLink>> {
        let conn = self.conn.lock();
        super::cross_links::candidates_for_node(&conn, node)
    }

    fn commit(&mut self) -> crate::Result<()> {
        // Auto-commit via rusqlite's default transaction semantics; no-op.
        Ok(())
    }

    fn begin_read_snapshot(&self) -> crate::Result<()> {
        // Re-entrant; see SqliteGraphStore::begin_read_snapshot for rationale.
        let mut depth = self.snapshot_depth.lock();
        if *depth == 0 {
            self.conn.lock().execute_batch("BEGIN DEFERRED")?;
        }
        *depth += 1;
        Ok(())
    }

    fn end_read_snapshot(&self) -> crate::Result<()> {
        let mut depth = self.snapshot_depth.lock();
        if *depth == 0 {
            return Ok(());
        }
        *depth -= 1;
        if *depth == 0 {
            let conn = self.conn.lock();
            match conn.execute_batch("COMMIT") {
                Ok(()) => Ok(()),
                Err(err) if err.to_string().contains("no transaction") => Ok(()),
                Err(err) => Err(err.into()),
            }
        } else {
            Ok(())
        }
    }

    fn insert_commentary(&mut self, entry: CommentaryEntry) -> crate::Result<()> {
        if !has_complete_provenance(&entry.provenance) {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "commentary entry is missing required provenance fields"
            )));
        }
        if entry.text.is_empty() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "commentary entry text must not be empty"
            )));
        }

        let node_key = entry.node_id.to_string();
        let generated_at = entry
            .provenance
            .generated_at
            .format(&Rfc3339)
            .map_err(|e| crate::Error::Other(anyhow::anyhow!("invalid timestamp: {e}")))?;

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO commentary
                (node_id, text, source_content_hash, pass_id, model_identity, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET
                 text = excluded.text,
                 source_content_hash = excluded.source_content_hash,
                 pass_id = excluded.pass_id,
                 model_identity = excluded.model_identity,
                 generated_at = excluded.generated_at",
            params![
                node_key,
                entry.text,
                entry.provenance.source_content_hash,
                entry.provenance.pass_id,
                entry.provenance.model_identity,
                generated_at,
            ],
        )?;
        Ok(())
    }

    fn commentary_for(&self, node: NodeId) -> crate::Result<Option<CommentaryEntry>> {
        let node_key = node.to_string();
        let conn = self.conn.lock();
        let row = conn
            .query_row(
                "SELECT text, source_content_hash, pass_id, model_identity, generated_at
                 FROM commentary
                 WHERE node_id = ?1",
                params![node_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;

        match row {
            None => Ok(None),
            Some((text, hash, pass_id, model_identity, generated_at)) => {
                let generated_at = OffsetDateTime::parse(&generated_at, &Rfc3339).map_err(|e| {
                    crate::Error::Other(anyhow::anyhow!(
                        "invalid stored generated_at timestamp: {e}"
                    ))
                })?;
                Ok(Some(CommentaryEntry {
                    node_id: node,
                    text,
                    provenance: CommentaryProvenance {
                        source_content_hash: hash,
                        pass_id,
                        model_identity,
                        generated_at,
                    },
                }))
            }
        }
    }

    fn insert_note(
        &mut self,
        note: crate::overlay::AgentNote,
    ) -> crate::Result<crate::overlay::AgentNote> {
        self.insert_note_impl(note)
    }

    fn query_notes(
        &self,
        query: crate::overlay::AgentNoteQuery,
    ) -> crate::Result<Vec<crate::overlay::AgentNote>> {
        self.query_notes_impl_trait(query)
    }

    fn note_by_id(&self, note_id: &str) -> crate::Result<Option<crate::overlay::AgentNote>> {
        self.note_by_id_impl(note_id)
    }

    fn link_note(&mut self, from_note: &str, to_note: &str, actor: &str) -> crate::Result<()> {
        self.link_note_impl(from_note, to_note, actor)
    }

    fn supersede_note(
        &mut self,
        old_note: &str,
        replacement: crate::overlay::AgentNote,
        actor: &str,
    ) -> crate::Result<crate::overlay::AgentNote> {
        self.supersede_note_impl(old_note, replacement, actor)
    }

    fn forget_note(
        &mut self,
        note_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> crate::Result<()> {
        self.forget_note_impl(note_id, actor, reason)
    }

    fn verify_note(
        &mut self,
        note_id: &str,
        actor: &str,
        graph_revision: Option<u64>,
    ) -> crate::Result<crate::overlay::AgentNote> {
        self.verify_note_impl(note_id, actor, graph_revision)
    }

    fn mark_stale_notes(&mut self, stale_note_ids: &[String], actor: &str) -> crate::Result<usize> {
        self.mark_stale_notes_impl(stale_note_ids, actor)
    }

    fn note_counts(&self) -> crate::Result<crate::overlay::AgentNoteCounts> {
        self.note_counts_impl()
    }

    fn prune_orphans(&mut self, live_nodes: &[NodeId]) -> crate::Result<usize> {
        let live: HashSet<String> = live_nodes.iter().map(|id| id.to_string()).collect();

        let conn = self.conn.lock();

        // Prune commentary.
        let mut stmt = conn.prepare("SELECT node_id FROM commentary")?;
        let existing: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut removed = 0usize;
        for stored in existing {
            if !live.contains(&stored) {
                conn.execute("DELETE FROM commentary WHERE node_id = ?1", params![stored])?;
                removed += 1;
            }
        }

        // Prune cross-links (records audit rows with reason `source_deleted`).
        removed += super::cross_links::prune_orphans(&conn, &live)?;

        Ok(removed)
    }

    fn all_candidates(
        &self,
        tier: Option<&str>,
    ) -> crate::Result<Vec<crate::overlay::OverlayLink>> {
        let conn = self.conn.lock();
        super::cross_links::all_candidates(&conn, tier)
    }

    fn mark_candidate_rejected(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: crate::overlay::OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        super::cross_links::mark_rejected(&conn, from, to, kind, reviewer)
    }

    fn mark_candidate_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: crate::overlay::OverlayEdgeKind,
        reviewer: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        super::cross_links::mark_pending(&conn, from, to, kind, reviewer)
    }

    fn mark_candidate_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: crate::overlay::OverlayEdgeKind,
        reviewer: &str,
        graph_edge_id: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        super::cross_links::mark_promoted(&conn, from, to, kind, reviewer, graph_edge_id)
    }

    fn compactable_commentary_stats(
        &self,
        policy: &crate::pipeline::maintenance::CompactPolicy,
    ) -> crate::Result<crate::pipeline::maintenance::CompactStats> {
        let cutoff_str =
            crate::pipeline::maintenance::retention_cutoff(policy.commentary_retention_days())?;

        let conn = self.conn.lock();
        let count: usize = conn.query_row(
            "SELECT COUNT(*) FROM commentary WHERE generated_at < ?1",
            params![cutoff_str],
            |row| row.get(0),
        )?;

        Ok(crate::pipeline::maintenance::CompactStats {
            compactable_commentary: count,
            compactable_cross_links: 0,
            repair_log_entries_beyond_window: 0,
            last_compaction_timestamp: None,
        })
    }

    fn compact_commentary(
        &mut self,
        policy: &crate::pipeline::maintenance::CompactPolicy,
    ) -> crate::Result<usize> {
        let cutoff_str =
            crate::pipeline::maintenance::retention_cutoff(policy.commentary_retention_days())?;

        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM commentary WHERE generated_at < ?1",
            params![cutoff_str],
        )?;

        Ok(deleted)
    }

    fn compactable_cross_link_stats(
        &self,
        policy: &crate::pipeline::maintenance::CompactPolicy,
    ) -> crate::Result<crate::pipeline::maintenance::CompactStats> {
        let cutoff_str =
            crate::pipeline::maintenance::retention_cutoff(policy.audit_retention_days())?;

        let conn = self.conn.lock();
        let count: usize = conn.query_row(
            "SELECT COUNT(*) FROM cross_link_audit WHERE state IN ('promoted', 'rejected') AND event_at < ?1",
            params![cutoff_str],
            |row| row.get(0),
        )?;

        Ok(crate::pipeline::maintenance::CompactStats {
            compactable_commentary: 0,
            compactable_cross_links: count,
            repair_log_entries_beyond_window: 0,
            last_compaction_timestamp: None,
        })
    }

    fn compact_cross_links(
        &mut self,
        policy: &crate::pipeline::maintenance::CompactPolicy,
    ) -> crate::Result<usize> {
        let cutoff_str =
            crate::pipeline::maintenance::retention_cutoff(policy.audit_retention_days())?;

        let conn = self.conn.lock();
        // Delete old audit rows that are promoted or rejected.
        let deleted = conn.execute(
            "DELETE FROM cross_link_audit WHERE state IN ('promoted', 'rejected') AND event_at < ?1",
            params![cutoff_str],
        )?;

        Ok(deleted)
    }

    fn cross_link_audit_count(&self) -> crate::Result<usize> {
        let conn = self.conn.lock();
        Ok(
            conn.query_row("SELECT COUNT(*) FROM cross_link_audit", [], |row| {
                row.get(0)
            })?,
        )
    }
}

impl SqliteOverlayStore {
    /// Return every commentary entry currently stored.
    pub fn all_commentary_entries(&self) -> crate::Result<Vec<CommentaryEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT node_id, text, source_content_hash, pass_id, model_identity, generated_at
             FROM commentary
             ORDER BY node_id",
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

        rows.into_iter()
            .map(
                |(node_id, text, source_content_hash, pass_id, model_identity, generated_at)| {
                    let node_id = NodeId::from_str(&node_id).map_err(|err| {
                        crate::Error::Other(anyhow::anyhow!(
                            "invalid stored commentary node_id `{node_id}`: {err}"
                        ))
                    })?;
                    let generated_at =
                        OffsetDateTime::parse(&generated_at, &Rfc3339).map_err(|err| {
                            crate::Error::Other(anyhow::anyhow!(
                                "invalid stored generated_at timestamp: {err}"
                            ))
                        })?;
                    Ok(CommentaryEntry {
                        node_id,
                        text,
                        provenance: CommentaryProvenance {
                            source_content_hash,
                            pass_id,
                            model_identity,
                            generated_at,
                        },
                    })
                },
            )
            .collect()
    }
}
