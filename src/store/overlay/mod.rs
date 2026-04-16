//! Sqlite-backed overlay store for commentary entries.
//!
//! The overlay database is physically separate from the canonical graph
//! store: commentary lives at `.synrepo/overlay/overlay.db`; the graph lives
//! at `.synrepo/graph/nodes.db`. No code path writes commentary data to the
//! graph, or graph data to the overlay.

mod commentary;
mod cross_link_audit;
mod cross_links;
mod findings;
mod schema;

#[cfg(test)]
mod findings_tests;
#[cfg(test)]
mod tests;

pub use commentary::derive_freshness;
pub use cross_link_audit::AuditRow;
pub use cross_links::{CrossLinkHashRow, CrossLinkStateCounts, PendingPromotionRow};
pub use findings::{
    candidate_pass_suffix, compare_score_desc, format_candidate_id, parse_cross_link_freshness,
    parse_overlay_edge_kind, CrossLinkFinding, FindingsFilter, CANDIDATE_ID_PASS_SUFFIX_LEN,
};

/// Current overlay schema version shipped by this binary (v1: commentary-only;
/// v2: commentary + cross-links).
pub const CURRENT_SCHEMA_VERSION: u32 = schema::CURRENT_SCHEMA_VERSION;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
};

use schema::init_schema;

const OVERLAY_DB_FILENAME: &str = "overlay.db";

/// Sqlite-backed overlay store rooted at `.synrepo/overlay/`.
pub struct SqliteOverlayStore {
    pub(super) conn: Mutex<Connection>,
    /// Re-entrant read-snapshot depth counter. See
    /// [`crate::store::sqlite::SqliteGraphStore::snapshot_depth`] for the
    /// semantics; the overlay mirrors them.
    pub(super) snapshot_depth: Mutex<usize>,
}

impl SqliteOverlayStore {
    /// Open or create the overlay store inside `.synrepo/overlay/`.
    ///
    /// Creates the directory and the `overlay.db` file on first use; the
    /// store is otherwise lazy (never materialized during `synrepo init`).
    pub fn open(overlay_dir: &Path) -> crate::Result<Self> {
        fs::create_dir_all(overlay_dir)?;
        Self::open_db(&overlay_dir.join(OVERLAY_DB_FILENAME))
    }

    /// Open or create the overlay store at an explicit sqlite database path.
    pub fn open_db(db_path: &Path) -> crate::Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        init_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            snapshot_depth: Mutex::new(0),
        })
    }

    /// Open an existing overlay store without creating a new database.
    ///
    /// Returns an error if the overlay database file does not yet exist.
    pub fn open_existing(overlay_dir: &Path) -> crate::Result<Self> {
        let db_path = Self::db_path(overlay_dir);
        if !db_path.exists() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "overlay store is not materialized at {}",
                db_path.display()
            )));
        }

        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        init_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            snapshot_depth: Mutex::new(0),
        })
    }

    /// Absolute path of the sqlite file used by the overlay store.
    pub fn db_path(overlay_dir: &Path) -> PathBuf {
        overlay_dir.join(OVERLAY_DB_FILENAME)
    }

    /// Return the number of `generated` events in the cross-link audit trail.
    /// Each event corresponds to one LLM generation call.
    pub fn cross_link_generation_count(&self) -> crate::Result<usize> {
        let conn = self.conn.lock();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM cross_link_audit WHERE event_kind = 'generated'",
            [],
            |row| row.get::<_, usize>(0),
        )?)
    }

    /// Return the number of commentary rows currently stored.
    pub fn commentary_count(&self) -> crate::Result<usize> {
        let conn = self.conn.lock();
        Ok(
            conn.query_row("SELECT COUNT(*) FROM commentary", [], |row| {
                row.get::<_, usize>(0)
            })?,
        )
    }

    /// Return every `(node_id, source_content_hash)` pair from the commentary
    /// table. Used by the repair loop to classify stale entries without
    /// pulling full provenance for rows that do not need refresh.
    pub fn commentary_hashes(&self) -> crate::Result<Vec<(String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT node_id, source_content_hash FROM commentary")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return active cross-link candidates as simple row tuples.
    /// Used by the handoffs surface to display pending candidates.
    pub fn active_cross_links(&self) -> crate::Result<Vec<(String, String, String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT from_node, to_node, confidence_tier, rationale
             FROM cross_links
             WHERE state = 'active'",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3).unwrap_or_default(),
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
