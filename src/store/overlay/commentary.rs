//! Commentary CRUD and freshness derivation for `SqliteOverlayStore`.

use std::collections::HashSet;

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
}
