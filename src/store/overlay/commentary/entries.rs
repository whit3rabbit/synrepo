//! Read helpers for commentary entries, exposed directly on `SqliteOverlayStore`.

use std::str::FromStr;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{CommentaryEntry, CommentaryProvenance};

use super::super::SqliteOverlayStore;

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
