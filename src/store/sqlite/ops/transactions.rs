//! Transaction and snapshot operations for SqliteGraphStore.

use super::SqliteGraphStore;

/// Begin a deferred transaction.
pub fn begin(store: &mut SqliteGraphStore) -> crate::Result<()> {
    store.conn.lock().execute_batch("BEGIN DEFERRED")?;
    Ok(())
}

/// Commit the current transaction.
pub fn commit(store: &mut SqliteGraphStore) -> crate::Result<()> {
    store.conn.lock().execute_batch("COMMIT")?;
    Ok(())
}

/// Rollback the current transaction.
pub fn rollback(store: &mut SqliteGraphStore) -> crate::Result<()> {
    store.conn.lock().execute_batch("ROLLBACK")?;
    Ok(())
}

/// Begin a read snapshot. Re-entrant: only the outermost begin issues
/// BEGIN DEFERRED. Inner begins share the outer snapshot so callers
/// composing wrapped operations don't trip SQLite's "transaction within
/// a transaction" error.
pub fn begin_read_snapshot(store: &SqliteGraphStore) -> crate::Result<()> {
    let mut depth = store.snapshot_depth.lock();
    if *depth == 0 {
        store.conn.lock().execute_batch("BEGIN DEFERRED")?;
    }
    *depth += 1;
    Ok(())
}

/// End a read snapshot. Commit only when the outermost end is called.
pub fn end_read_snapshot(store: &SqliteGraphStore) -> crate::Result<()> {
    let mut depth = store.snapshot_depth.lock();
    if *depth == 0 {
        // end-without-begin: treat as no-op so the `with_*` helper's
        // error-path cleanup can't mask the caller's original error.
        return Ok(());
    }
    *depth -= 1;
    if *depth == 0 {
        let conn = store.conn.lock();
        match conn.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            // Why: SQLite returns the generic SQLITE_ERROR code for "COMMIT
            // outside a transaction", so the error code alone cannot
            // distinguish this case. The string match is intentional. If
            // SQLite changes the message phrasing, this swallow stops working
            // and a legitimate error surfaces — a fail-loud regression that
            // we would catch at test time.
            Err(err) if err.to_string().contains("no transaction") => Ok(()),
            Err(err) => Err(err.into()),
        }
    } else {
        Ok(())
    }
}
