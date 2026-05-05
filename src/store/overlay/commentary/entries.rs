//! Read helpers for commentary entries, exposed directly on `SqliteOverlayStore`.

use std::str::FromStr;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{CommentaryEntry, CommentaryProvenance};

use super::super::SqliteOverlayStore;

impl SqliteOverlayStore {
    /// Return oldest and newest commentary generation timestamps.
    pub fn commentary_generated_at_bounds(
        &self,
    ) -> crate::Result<Option<(OffsetDateTime, OffsetDateTime)>> {
        let conn = self.conn.lock();
        let (oldest, newest): (Option<String>, Option<String>) = conn.query_row(
            "SELECT MIN(generated_at), MAX(generated_at) FROM commentary",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )?;

        let (Some(oldest), Some(newest)) = (oldest, newest) else {
            return Ok(None);
        };
        let oldest = OffsetDateTime::parse(&oldest, &Rfc3339).map_err(|err| {
            crate::Error::Other(anyhow::anyhow!(
                "invalid stored generated_at timestamp: {err}"
            ))
        })?;
        let newest = OffsetDateTime::parse(&newest, &Rfc3339).map_err(|err| {
            crate::Error::Other(anyhow::anyhow!(
                "invalid stored generated_at timestamp: {err}"
            ))
        })?;
        Ok(Some((oldest, newest)))
    }

    /// Stream `(node_id, source_content_hash)` pairs from commentary rows.
    ///
    /// This intentionally avoids loading commentary bodies for status and
    /// repair freshness scans, where only provenance hashes matter.
    pub fn scan_commentary_hashes<F>(&self, mut visit: F) -> crate::Result<usize>
    where
        F: FnMut(&str, &str) -> crate::Result<()>,
    {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT node_id, source_content_hash FROM commentary ORDER BY node_id")?;
        let mut rows = stmt.query([])?;
        let mut count = 0usize;
        while let Some(row) = rows.next()? {
            let node_id: String = row.get(0)?;
            let source_content_hash: String = row.get(1)?;
            visit(&node_id, &source_content_hash)?;
            count += 1;
        }
        Ok(count)
    }

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
