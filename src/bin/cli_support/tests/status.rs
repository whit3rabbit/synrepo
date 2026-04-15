use std::process::Command;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::core::ids::{NodeId, SymbolNodeId};
use synrepo::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;
use time::OffsetDateTime;

use super::super::commands::status_output;
use super::support::seed_graph;

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

    let out = status_output(repo.path(), true, false).unwrap();
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(json, serde_json::json!({ "initialized": false }));
}

#[test]
fn status_not_initialized_human() {
    let repo = tempdir().unwrap();
    write_malformed_config(repo.path());

    let out = status_output(repo.path(), false, false).unwrap();
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

    let json: serde_json::Value =
        serde_json::from_str(status_output(repo.path(), true, false).unwrap().trim()).unwrap();
    assert_eq!(json["initialized"], true);
    assert_eq!(json["graph"]["file_nodes"], 1);
    assert_eq!(json["graph"]["symbol_nodes"], 1);
    assert_eq!(json["graph"]["concept_nodes"], 1);
    // Mode for seed_graph is the default (auto).
    assert_eq!(json["mode"], "auto");

    let text = status_output(repo.path(), false, false).unwrap();
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

    let json: serde_json::Value =
        serde_json::from_str(status_output(repo.path(), true, false).unwrap().trim()).unwrap();
    assert_eq!(
        json["writer_lock"],
        serde_json::Value::String(format!("held_by_pid_{pid}")),
        "expected writer_lock held_by_pid_{pid}, full json: {json}"
    );

    let text = status_output(repo.path(), false, false).unwrap();
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

    let text = status_output(repo.path(), false, false).unwrap();
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

    let text = status_output(repo.path(), false, false).unwrap();
    assert!(
        text.contains("commentary:   1 entries (graph unreadable)"),
        "expected `1 entries (graph unreadable)` line, got: {text}"
    );
}

#[test]
fn status_recent_activity_json_round_trip() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let json: serde_json::Value =
        serde_json::from_str(status_output(repo.path(), true, true).unwrap().trim()).unwrap();
    // recent=true must produce a JSON array (possibly empty) rather than null.
    assert!(
        json["recent_activity"].is_array(),
        "expected recent_activity to be an array, got: {}",
        json["recent_activity"]
    );

    let null_json: serde_json::Value =
        serde_json::from_str(status_output(repo.path(), true, false).unwrap().trim()).unwrap();
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
    let text = status_output(repo.path(), false, false).unwrap();
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

    let text = status_output(repo.path(), false, false).unwrap();
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

    let text = status_output(repo.path(), false, false).unwrap();
    assert!(
        text.contains("graph is current"),
        "expected `graph is current` next-step line, got: {text}"
    );
}
