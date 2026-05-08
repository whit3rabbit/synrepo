use std::panic::{catch_unwind, AssertUnwindSafe};

use rusqlite::params;
use tempfile::tempdir;

use super::super::{with_write_transaction, SqliteOverlayStore};

#[test]
fn write_transaction_rolls_back_when_closure_panics() {
    let dir = tempdir().unwrap();
    let store = SqliteOverlayStore::open(dir.path()).unwrap();

    let panic_result = catch_unwind(AssertUnwindSafe(|| {
        let conn = store.conn.lock();
        let _ = with_write_transaction(&conn, |_conn| -> crate::Result<()> {
            panic!("boom");
        });
    }));
    assert!(panic_result.is_err());

    let conn = store.conn.lock();
    with_write_transaction(&conn, |conn| {
        conn.execute(
            "INSERT INTO commentary
             (node_id, text, source_content_hash, pass_id, model_identity, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["node-a", "text", "hash", "pass", "model", "now"],
        )?;
        Ok(())
    })
    .unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commentary", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
