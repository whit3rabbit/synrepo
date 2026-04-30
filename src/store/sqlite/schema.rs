use rusqlite::Connection;

use super::values::row_usize;

pub(super) fn init_schema(conn: &Connection) -> crate::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS files (
            id                TEXT PRIMARY KEY,
            root_id           TEXT NOT NULL DEFAULT 'primary',
            path              TEXT NOT NULL,
            last_observed_rev INTEGER,
            data              TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_files_root_path ON files(root_id, path);

        CREATE TABLE IF NOT EXISTS symbols (
            id                TEXT PRIMARY KEY,
            file_id           TEXT NOT NULL,
            qualified_name    TEXT NOT NULL,
            kind              TEXT NOT NULL,
            body_hash         TEXT,
            first_seen_rev    TEXT,
            last_modified_rev TEXT,
            last_observed_rev INTEGER,
            retired_at_rev    INTEGER,
            data              TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE INDEX IF NOT EXISTS idx_symbols_file_id   ON symbols(file_id);
        CREATE INDEX IF NOT EXISTS idx_symbols_body_hash ON symbols(body_hash);

        CREATE TABLE IF NOT EXISTS concepts (
            id                TEXT PRIMARY KEY,
            path              TEXT NOT NULL UNIQUE,
            last_observed_rev INTEGER,
            data              TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS edges (
            id                TEXT PRIMARY KEY,
            from_node_id      TEXT NOT NULL,
            to_node_id        TEXT NOT NULL,
            kind              TEXT NOT NULL,
            owner_file_id     TEXT,
            last_observed_rev INTEGER,
            retired_at_rev    INTEGER,
            data              TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE INDEX IF NOT EXISTS idx_edges_from_kind ON edges(from_node_id, kind);
        CREATE INDEX IF NOT EXISTS idx_edges_to_kind   ON edges(to_node_id,   kind);

        CREATE TABLE IF NOT EXISTS edge_drift (
            edge_id     TEXT NOT NULL,
            revision    TEXT NOT NULL,
            drift_score REAL NOT NULL,
            PRIMARY KEY (edge_id, revision)
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS file_fingerprints (
            file_node_id TEXT NOT NULL,
            revision     TEXT NOT NULL,
            fingerprint  TEXT NOT NULL,
            PRIMARY KEY (file_node_id, revision)
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS compile_revisions (
            revision_id INTEGER PRIMARY KEY,
            created_at  TEXT NOT NULL
        );
        ",
    )?;

    Ok(())
}

pub(super) fn count_rows(conn: &Connection, table: &str) -> crate::Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    Ok(conn.query_row(&sql, [], |row| row_usize(row, 0))?)
}
