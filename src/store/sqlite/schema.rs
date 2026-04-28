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
            id TEXT PRIMARY KEY,
            root_id TEXT NOT NULL DEFAULT 'primary',
            path TEXT NOT NULL,
            data TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_files_root_path ON files(root_id, path);

        CREATE TABLE IF NOT EXISTS symbols (
            id TEXT PRIMARY KEY,
            file_id TEXT NOT NULL,
            qualified_name TEXT NOT NULL,
            kind TEXT NOT NULL,
            data TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE INDEX IF NOT EXISTS idx_symbols_file_id ON symbols(file_id);

        CREATE TABLE IF NOT EXISTS concepts (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            data TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS edges (
            id TEXT PRIMARY KEY,
            from_node_id TEXT NOT NULL,
            to_node_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            data TEXT NOT NULL
        ) WITHOUT ROWID;

        CREATE INDEX IF NOT EXISTS idx_edges_from_kind ON edges(from_node_id, kind);
        CREATE INDEX IF NOT EXISTS idx_edges_to_kind ON edges(to_node_id, kind);

        CREATE TABLE IF NOT EXISTS edge_drift (
            edge_id TEXT NOT NULL,
            revision TEXT NOT NULL,
            drift_score REAL NOT NULL,
            PRIMARY KEY (edge_id, revision)
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS file_fingerprints (
            file_node_id TEXT NOT NULL,
            revision TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            PRIMARY KEY (file_node_id, revision)
        ) WITHOUT ROWID;
        ",
    )?;

    // compile_revisions: monotonic counter for observation-window tracking.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS compile_revisions (
            revision_id INTEGER PRIMARY KEY,
            created_at  TEXT NOT NULL,
            file_count  INTEGER NOT NULL DEFAULT 0,
            symbol_count INTEGER NOT NULL DEFAULT 0
        );",
    )?;

    // Additive migrations: duplicate-column errors are silently ignored.
    let migratables = vec![
        (
            "symbols",
            "first_seen_rev",
            "ALTER TABLE symbols ADD COLUMN first_seen_rev TEXT NULL",
        ),
        (
            "symbols",
            "last_modified_rev",
            "ALTER TABLE symbols ADD COLUMN last_modified_rev TEXT NULL",
        ),
        (
            "files",
            "last_observed_rev",
            "ALTER TABLE files ADD COLUMN last_observed_rev INTEGER NULL",
        ),
        (
            "files",
            "root_id",
            "ALTER TABLE files ADD COLUMN root_id TEXT NOT NULL DEFAULT 'primary'",
        ),
        (
            "symbols",
            "last_observed_rev",
            "ALTER TABLE symbols ADD COLUMN last_observed_rev INTEGER NULL",
        ),
        (
            "symbols",
            "retired_at_rev",
            "ALTER TABLE symbols ADD COLUMN retired_at_rev INTEGER NULL",
        ),
        (
            "edges",
            "owner_file_id",
            "ALTER TABLE edges ADD COLUMN owner_file_id INTEGER NULL",
        ),
        (
            "edges",
            "last_observed_rev",
            "ALTER TABLE edges ADD COLUMN last_observed_rev INTEGER NULL",
        ),
        (
            "edges",
            "retired_at_rev",
            "ALTER TABLE edges ADD COLUMN retired_at_rev INTEGER NULL",
        ),
        (
            "concepts",
            "last_observed_rev",
            "ALTER TABLE concepts ADD COLUMN last_observed_rev INTEGER NULL",
        ),
        (
            "symbols",
            "body_hash",
            "ALTER TABLE symbols ADD COLUMN body_hash TEXT NULL",
        ),
    ];

    for (table, column, sql) in migratables {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
        let mut has_column = false;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                has_column = true;
                break;
            }
        }
        if !has_column {
            conn.execute_batch(sql)?;
        }
    }

    // Index on body_hash for future filter/join queries.
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_symbols_body_hash ON symbols(body_hash)")?;

    conn.execute_batch(
        "UPDATE files SET root_id = COALESCE(json_extract(data, '$.root_id'), root_id, 'primary')",
    )?;
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_files_root_path ON files(root_id, path)",
    )?;

    // Backfill body_hash from the JSON blob for existing rows.
    conn.execute_batch(
        "UPDATE symbols SET body_hash = json_extract(data, '$.body_hash') WHERE body_hash IS NULL",
    )?;

    Ok(())
}

pub(super) fn count_rows(conn: &Connection, table: &str) -> crate::Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    Ok(conn.query_row(&sql, [], |row| row_usize(row, 0))?)
}
