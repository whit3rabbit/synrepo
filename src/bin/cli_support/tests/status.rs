use std::process::Command;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use synrepo::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;
use time::OffsetDateTime;

use super::super::commands::status_output;
use super::support::seed_graph;

/// Insert one commentary row with the given hash. Freshness classification in
/// the full path compares this hash against the containing file's current
/// `content_hash`; `seed_graph` seeds file `0x42` with hash `"abc123"`.
fn insert_commentary_row(store: &mut SqliteOverlayStore, node: NodeId, hash: &str) {
    store
        .insert_commentary(CommentaryEntry {
            node_id: node,
            text: "test commentary".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: hash.to_string(),
                pass_id: "test-commentary-v1".to_string(),
                model_identity: "test-model".to_string(),
                generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
            },
        })
        .unwrap();
}

/// `Config::load` returns `Ok(Self::default())` when `.synrepo/config.toml`
/// is missing, so the not-initialized branch only fires on a malformed TOML
/// file. Write one to drive that branch.
fn write_malformed_config(repo: &std::path::Path) {
    let synrepo_dir = Config::synrepo_dir(repo);
    std::fs::create_dir_all(&synrepo_dir).unwrap();
    std::fs::write(synrepo_dir.join("config.toml"), "not = valid = toml").unwrap();
}

#[test]
fn status_not_initialized_json() {
    let repo = tempdir().unwrap();
    write_malformed_config(repo.path());

    let out = status_output(repo.path(), true, false, false).unwrap();
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(json, serde_json::json!({ "initialized": false }));
}

#[test]
fn status_not_initialized_human() {
    let repo = tempdir().unwrap();
    write_malformed_config(repo.path());

    let out = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        out.contains("synrepo status: not initialized"),
        "expected not-initialized banner, got: {out}"
    );
    assert!(
        out.contains("Run `synrepo init`"),
        "expected init hint, got: {out}"
    );
}

#[test]
fn status_truly_uninitialized_json() {
    let repo = tempdir().unwrap();
    // No .synrepo directory created.
    let out = status_output(repo.path(), true, false, false).unwrap();
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(json, serde_json::json!({ "initialized": false }));
}

#[test]
fn status_truly_uninitialized_human() {
    let repo = tempdir().unwrap();
    // No .synrepo directory created.
    let out = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        out.contains("synrepo status: not initialized"),
        "expected not-initialized banner, got: {out}"
    );
    assert!(
        out.contains("Run `synrepo init`"),
        "expected init hint, got: {out}"
    );
}

#[test]
fn status_reports_graph_counts_after_bootstrap() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["initialized"], true);
    assert_eq!(json["graph"]["file_nodes"], 1);
    assert_eq!(json["graph"]["symbol_nodes"], 1);
    assert_eq!(json["graph"]["concept_nodes"], 1);
    // Mode for seed_graph is the default (auto).
    assert_eq!(json["mode"], "auto");

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("1 files  1 symbols  1 concepts"),
        "expected graph counts line, got: {text}"
    );
}

#[test]
fn status_reports_writer_lock_held_by_other() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut child = Command::new("sleep").arg("5").spawn().unwrap();
    let pid = child.id();
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string(&WriterOwnership {
            pid,
            acquired_at: "now".to_string(),
        })
        .unwrap(),
    )
    .unwrap();

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json["writer_lock"],
        serde_json::Value::String(format!("held_by_pid_{pid}")),
        "expected writer_lock held_by_pid_{pid}, full json: {json}"
    );

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains(&format!("held by pid {pid}")),
        "expected writer-lock line in text output, got: {text}"
    );
    // next_step should route to the writer-lock branch when the lock is held.
    assert!(
        text.contains("writer lock is held"),
        "expected writer-lock next-step hint, got: {text}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn status_overlay_cost_surfaces_query_failure() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    // Force overlay db to exist (so the "no overlay" early return is bypassed)
    // but be unreadable: open and close, then truncate.
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let _ = SqliteOverlayStore::open(&overlay_dir).unwrap();
    let db_path = SqliteOverlayStore::db_path(&overlay_dir);
    // Write garbage that is not a valid SQLite file so open_existing fails.
    // An empty file is treated as a fresh empty database by SQLite, which
    // does not exercise the failure branch.
    std::fs::write(&db_path, b"this is not a sqlite database header").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("overlay cost: unavailable"),
        "expected overlay-cost unavailable line, got: {text}"
    );
    // Critical: must not silently report "0 LLM calls" when the count query
    // failed. This locks down the intentional choice in overlay_cost_summary
    // to bubble errors instead of collapsing them to zero.
    assert!(
        !text.contains("overlay cost: no overlay") && !text.contains("overlay cost: 0 LLM calls"),
        "overlay-cost line must not collapse a query failure to zero, got: {text}"
    );
}

#[test]
fn status_commentary_coverage_graph_unreadable() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    let node = NodeId::Symbol(SymbolNodeId(0xabc));
    overlay
        .insert_commentary(CommentaryEntry {
            node_id: node,
            text: "Test commentary entry.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: "h1".to_string(),
                pass_id: "test-commentary-v1".to_string(),
                model_identity: "test-model".to_string(),
                generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
            },
        })
        .unwrap();

    // Render the graph store unreadable by removing the directory entirely
    // so SqliteGraphStore::open_existing fails.
    let graph_dir = synrepo_dir.join("graph");
    std::fs::remove_dir_all(&graph_dir).unwrap();

    // The graph-unreadable branch only fires in the full (freshness) path;
    // the default cheap path never opens the graph store.
    let text = status_output(repo.path(), false, false, true).unwrap();
    assert!(
        text.contains("commentary:   1 entries (graph unreadable)"),
        "expected `1 entries (graph unreadable)` line, got: {text}"
    );
}

#[test]
fn status_recent_activity_json_round_trip() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, true, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    // recent=true must produce a JSON array (possibly empty) rather than null.
    assert!(
        json["recent_activity"].is_array(),
        "expected recent_activity to be an array, got: {}",
        json["recent_activity"]
    );

    let null_json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert!(
        null_json["recent_activity"].is_null(),
        "expected null recent_activity when recent=false, got: {}",
        null_json["recent_activity"]
    );
}

#[test]
fn status_next_step_routes_to_unknown_reconcile() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    // Fresh seed_graph leaves no reconcile-state.json, so reconcile_health is
    // Unknown and next_step should suggest the first reconcile pass.
    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("next step:    run `synrepo reconcile` to do the first graph pass"),
        "expected first-reconcile next-step line, got: {text}"
    );
}

#[test]
fn status_next_step_routes_to_writer_lock_when_held() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut child = Command::new("sleep").arg("5").spawn().unwrap();
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string(&WriterOwnership {
            pid: child.id(),
            acquired_at: "now".to_string(),
        })
        .unwrap(),
    )
    .unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("writer lock is held"),
        "expected writer-lock-held next-step line, got: {text}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn status_next_step_routes_to_current_when_reconcile_completed() {
    use synrepo::pipeline::structural::CompileSummary;
    use synrepo::pipeline::watch::{persist_reconcile_state, ReconcileOutcome};

    let repo = tempdir().unwrap();
    bootstrap(repo.path(), None).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Completed(CompileSummary::default()),
        0,
    );

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("graph is current"),
        "expected `graph is current` next-step line, got: {text}"
    );
}

#[test]
fn status_reports_corrupt_reconcile_state() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("reconcile-state.json"), b"not valid json").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(text.contains("reconcile:    corrupt"), "got: {text}");

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["reconcile_health"], "corrupt");
}

#[test]
fn status_reports_corrupt_writer_lock() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("writer.lock"), b"not valid json").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(text.contains("writer lock:  corrupt"), "got: {text}");

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["writer_lock"], "corrupt");
}

#[test]
fn status_reports_corrupt_watch_state() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("watch-daemon.json"), b"not valid json").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(text.contains("watch:        corrupt"), "got: {text}");

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert!(json["watch"].as_str().unwrap().contains("corrupt"));
}

/// Default status must show the commentary row count but must NOT compute
/// per-row freshness. JSON emits `commentary_coverage.fresh` as null so
/// consumers can distinguish "not computed" from "zero fresh".
#[test]
fn status_default_skips_freshness_scan() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    // 5 rows: 2 pointing at real graph nodes, 3 at nonexistent node ids.
    // Freshness does not matter for the cheap path.
    insert_commentary_row(&mut overlay, NodeId::File(FileNodeId(0x42)), "abc123");
    insert_commentary_row(&mut overlay, NodeId::Symbol(SymbolNodeId(0x24)), "abc123");
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead01)),
        "stale",
    );
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead02)),
        "stale",
    );
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead03)),
        "stale",
    );
    drop(overlay);

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("commentary:   5 entries"),
        "expected cheap-path `5 entries`, got: {text}"
    );
    assert!(
        !text.contains("fresh /"),
        "default path must not render the `fresh / total` freshness summary, got: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["commentary_coverage"]["total"], 5);
    assert!(
        json["commentary_coverage"]["fresh"].is_null(),
        "cheap path must emit fresh: null, got: {}",
        json["commentary_coverage"]
    );
}

/// `--full` must run the freshness scan and report actual counts.
#[test]
fn status_full_computes_freshness() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    // seed_graph produces file 0x42 with content_hash "abc123"; the symbol
    // 0x24 sits in that file. Both resolve to file-hash "abc123" via
    // resolve_commentary_node, so commentary with source_content_hash
    // "abc123" is fresh against them.
    insert_commentary_row(&mut overlay, NodeId::File(FileNodeId(0x42)), "abc123");
    insert_commentary_row(&mut overlay, NodeId::Symbol(SymbolNodeId(0x24)), "abc123");
    // Three stale rows: same file hash but wrong stored hash + missing node
    // ids. resolve returns None for missing, and hash mismatch for the wrong
    // hash case.
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead01)),
        "abc123",
    );
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead02)),
        "abc123",
    );
    drop(overlay);

    let text = status_output(repo.path(), false, false, true).unwrap();
    assert!(
        text.contains("2 fresh / 4 total nodes with commentary"),
        "expected full-path freshness summary, got: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, true)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["commentary_coverage"]["total"], 4);
    assert_eq!(json["commentary_coverage"]["fresh"], 2);
}

/// The cheap path must stay cheap: seeding 1000 commentary rows should not
/// make default status slow. This is a regression guard against accidentally
/// reintroducing an O(N) scan under the default flag. The bound is generous
/// (1 second on CI-class hardware) because it guards complexity, not a perf SLA.
#[test]
fn status_default_with_1000_commentary_rows_completes_quickly() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    for i in 0..1000_u64 {
        insert_commentary_row(
            &mut overlay,
            NodeId::Symbol(SymbolNodeId(0x10_0000 + i)),
            "stale",
        );
    }
    drop(overlay);

    let start = std::time::Instant::now();
    let text = status_output(repo.path(), false, false, false).unwrap();
    let elapsed = start.elapsed();

    assert!(
        text.contains("commentary:   1000 entries"),
        "expected `1000 entries`, got: {text}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "default status must stay cheap with 1000 commentary rows, took {elapsed:?}"
    );
}
