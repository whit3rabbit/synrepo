use rusqlite::Connection;

pub(super) fn init_schema(conn: &Connection) -> crate::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            data TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS symbols (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL,
            qualified_name TEXT NOT NULL,
            kind TEXT NOT NULL,
            data TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_symbols_file_id ON symbols(file_id);

        CREATE TABLE IF NOT EXISTS concepts (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            data TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS edges (
            id INTEGER PRIMARY KEY,
            from_node_id TEXT NOT NULL,
            to_node_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            data TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_edges_from_kind ON edges(from_node_id, kind);
        CREATE INDEX IF NOT EXISTS idx_edges_to_kind ON edges(to_node_id, kind);
        ",
    )?;

    // Additive migration: symbol-scoped revision columns (symbol-last-change-v1).
    // Safe to re-run: "duplicate column" errors are silently ignored.
    let migratables = [
        "ALTER TABLE symbols ADD COLUMN first_seen_rev TEXT NULL",
        "ALTER TABLE symbols ADD COLUMN last_modified_rev TEXT NULL",
    ];
    for sql in &migratables {
        // sqlite3_stricmp-based match for the specific "duplicate column" error.
        match conn.execute_batch(sql) {
            Ok(()) => {}
            Err(err) if err.to_string().contains("duplicate column") => {}
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

pub(super) fn count_rows(conn: &Connection, table: &str) -> crate::Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    Ok(conn.query_row(&sql, [], |row| row.get::<_, usize>(0))?)
}
