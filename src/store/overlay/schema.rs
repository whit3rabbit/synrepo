//! Schema bootstrap for the overlay SQLite database.
//!
//! The overlay database lives at `.synrepo/overlay/overlay.db` and is
//! physically separate from the canonical graph store at
//! `.synrepo/graph/nodes.db`. No graph tables (`files`, `symbols`,
//! `concepts`, `edges`) are ever created here.

use rusqlite::Connection;

pub(super) fn init_schema(conn: &Connection) -> crate::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS commentary (
            id INTEGER PRIMARY KEY,
            node_id TEXT NOT NULL UNIQUE,
            text TEXT NOT NULL,
            source_content_hash TEXT NOT NULL,
            pass_id TEXT NOT NULL,
            model_identity TEXT NOT NULL,
            generated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_commentary_node_id ON commentary(node_id);
        ",
    )?;
    Ok(())
}
