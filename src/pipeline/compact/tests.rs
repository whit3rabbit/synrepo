use super::*;
use tempfile::tempdir;

#[test]
fn rotate_repair_log_creates_header_with_summary() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();

    // Write a sample repair log.
    let log_path = state_dir.join("repair-log.jsonl");
    let now = OffsetDateTime::now_utc();
    let old_timestamp = now - time::Duration::days(60);
    let recent_timestamp = now - time::Duration::days(5);

    let old_ts_str = old_timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let recent_ts_str = recent_timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    std::fs::write(
        &log_path,
        format!(
            r#"{{"timestamp":"{}","surface":"graph","action":"retire_edges"}}
{{"timestamp":"{}","surface":"overlay","action":"prune_orphans"}}
"#,
            old_ts_str, recent_ts_str
        ),
    )
    .unwrap();

    let summary = ops::rotate_repair_log(
        &synrepo_dir,
        &crate::pipeline::maintenance::CompactPolicy::Default,
    )
    .unwrap();
    assert_eq!(summary.repair_log_summarized, 1);

    // Verify new log has header and remaining entry.
    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.starts_with('#'), "should have summary header");
    assert!(
        content.contains(&recent_ts_str),
        "should retain recent entry"
    );
}

#[test]
fn rotate_repair_log_is_idempotent_on_already_compacted() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();

    // Write an already-compacted log with header.
    let log_path = state_dir.join("repair-log.jsonl");
    let now = OffsetDateTime::now_utc();
    let recent_timestamp = now - time::Duration::days(5);
    let recent_ts_str = recent_timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    std::fs::write(
        &log_path,
        format!(
            "# compacted 5 entries, graph=2, overlay=3\n\
{{\"timestamp\":\"{}\",\"surface\":\"overlay\",\"action\":\"prune_orphans\"}}\n",
            recent_ts_str
        ),
    )
    .unwrap();

    // Running rotation again should be a no-op (no entries beyond retention).
    let summary = ops::rotate_repair_log(
        &synrepo_dir,
        &crate::pipeline::maintenance::CompactPolicy::Default,
    )
    .unwrap();
    assert_eq!(summary.repair_log_summarized, 0);

    // The header should still be there.
    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        content.contains("compacted 5 entries"),
        "should preserve existing summary"
    );
}

#[test]
fn rotate_repair_log_uses_atomic_file_rewrite() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();

    // Write a log with entries that need compaction.
    let log_path = state_dir.join("repair-log.jsonl");
    let now = OffsetDateTime::now_utc();
    let old_timestamp = now - time::Duration::days(60);
    let old_ts_str = old_timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    std::fs::write(
        &log_path,
        format!(
            r#"{{"timestamp":"{}","surface":"graph","action":"retire_edges"}}
"#,
            old_ts_str
        ),
    )
    .unwrap();

    // Run rotation - this should atomically rewrite the file.
    let summary = ops::rotate_repair_log(
        &synrepo_dir,
        &crate::pipeline::maintenance::CompactPolicy::Default,
    )
    .unwrap();
    assert_eq!(summary.repair_log_summarized, 1);

    // The original file is replaced (no temp file left behind).
    assert!(!log_path.with_extension("jsonl.tmp").exists());
}

#[test]
fn wal_checkpoint_completes_without_error() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let graph_dir = synrepo_dir.join("graph");
    std::fs::create_dir_all(&graph_dir).unwrap();

    // Create a minimal graph db.
    let db_path = graph_dir.join("nodes.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute("CREATE TABLE IF NOT EXISTS nodes (id TEXT PRIMARY KEY)", [])
        .unwrap();
    conn.execute("INSERT INTO nodes (id) VALUES ('test')", [])
        .unwrap();
    drop(conn);

    let result = ops::wal_checkpoint(&synrepo_dir).unwrap();
    assert!(result, "WAL checkpoint should succeed");
}

#[test]
fn compact_plan_fills_estimates() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let config = Config::default();

    let plan = plan_compact(
        &synrepo_dir,
        &config,
        crate::pipeline::maintenance::CompactPolicy::Default,
    )
    .unwrap();
    // Just verify it doesn't panic and has actions.
    assert!(!plan.actions.is_empty()); // At least WAL checkpoint.
}

#[test]
fn execute_compact_full_pass() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let overlay_dir = synrepo_dir.join("overlay");
    let state_dir = synrepo_dir.join("state");
    let graph_dir = synrepo_dir.join("graph");
    std::fs::create_dir_all(&overlay_dir).unwrap();
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::create_dir_all(&graph_dir).unwrap();

    let graph_db = graph_dir.join("nodes.db");
    let conn = rusqlite::Connection::open(&graph_db).unwrap();
    conn.execute("CREATE TABLE IF NOT EXISTS nodes (id TEXT PRIMARY KEY)", [])
        .unwrap();
    conn.execute("INSERT INTO nodes (id) VALUES ('file_1')", [])
        .unwrap();
    conn.execute("INSERT INTO nodes (id) VALUES ('symbol_1')", [])
        .unwrap();
    drop(conn);

    let mut store = crate::store::overlay::SqliteOverlayStore::open(&overlay_dir).unwrap();
    let stale_node = crate::core::ids::NodeId::Symbol(crate::core::ids::SymbolNodeId(1));
    let old_timestamp = OffsetDateTime::now_utc() - time::Duration::days(60);
    store
        .insert_commentary(crate::overlay::CommentaryEntry {
            node_id: stale_node,
            text: "Stale commentary".to_string(),
            provenance: crate::overlay::CommentaryProvenance {
                source_content_hash: "h1".to_string(),
                pass_id: "test".to_string(),
                model_identity: "test".to_string(),
                generated_at: old_timestamp,
            },
        })
        .unwrap();
    store.commit().unwrap();

    let log_path = state_dir.join("repair-log.jsonl");
    let now = OffsetDateTime::now_utc();
    let old_ts = now - time::Duration::days(60);
    let old_ts_str = old_ts
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    std::fs::write(
        &log_path,
        format!(
            r#"{{"timestamp":"{}","surface":"graph","action":"retire_edges"}}
"#,
            old_ts_str
        ),
    )
    .unwrap();

    let conn = rusqlite::Connection::open(&graph_db).unwrap();
    let pre_file_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
        .unwrap();
    drop(conn);

    let config = Config::default();
    let plan = plan_compact(
        &synrepo_dir,
        &config,
        crate::pipeline::maintenance::CompactPolicy::Default,
    )
    .unwrap();
    let summary = execute_compact(
        &synrepo_dir,
        &plan,
        crate::pipeline::maintenance::CompactPolicy::Default,
    )
    .unwrap();

    assert!(summary.commentary_compacted >= 1 || summary.repair_log_summarized >= 1);

    let conn = rusqlite::Connection::open(&graph_db).unwrap();
    let post_file_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        pre_file_count, post_file_count,
        "graph rows must be preserved"
    );

    assert!(synrepo_dir.join("state/compact-state.json").exists());
}
