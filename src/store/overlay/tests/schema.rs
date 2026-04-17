//! Schema validation tests.

use crate::store::overlay::{SqliteOverlayStore, CURRENT_SCHEMA_VERSION};
use tempfile::tempdir;

#[test]
fn schema_does_not_create_graph_tables() {
    let dir = tempdir().unwrap();
    let store = SqliteOverlayStore::open(dir.path()).unwrap();

    let conn = store.conn.lock();
    for table in ["files", "symbols", "concepts", "edges"] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "overlay db must not contain graph table `{table}`"
        );
    }
    // And the overlay table does exist.
    let commentary_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='commentary'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(commentary_count, 1);
}

#[test]
fn open_existing_requires_prior_materialization() {
    let dir = tempdir().unwrap();
    // open_existing fails before any open/materialization.
    assert!(SqliteOverlayStore::open_existing(dir.path()).is_err());

    // After open() the db file exists; open_existing succeeds.
    drop(SqliteOverlayStore::open(dir.path()).unwrap());
    assert!(SqliteOverlayStore::open_existing(dir.path()).is_ok());
}

#[test]
fn schema_version_recorded_on_open() {
    let dir = tempdir().unwrap();
    let store = SqliteOverlayStore::open(dir.path()).unwrap();
    let conn = store.conn.lock();
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, CURRENT_SCHEMA_VERSION.to_string());
}
