use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::{handle_apply_anchor_edits, handle_prepare_edit_context};
#[cfg(unix)]
use crate::pipeline::writer::{
    hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
};
use crate::{bootstrap, config::Config, pipeline::context_metrics, surface::mcp::SynrepoState};

mod budget;
#[cfg(unix)]
mod path_safety;
mod watch;

fn state_with_files(files: &[(&str, &str)]) -> (tempfile::TempDir, SynrepoState) {
    let dir = tempdir().unwrap();
    for (path, content) in files {
        let abs = dir.path().join(path);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(abs, content).unwrap();
    }
    bootstrap::bootstrap(dir.path(), None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(dir.path()).unwrap(),
        repo_root: dir.path().to_path_buf(),
    };
    (dir, state)
}

fn prepare(state: &SynrepoState, params: serde_json::Value) -> serde_json::Value {
    let params = serde_json::from_value(params).unwrap();
    let out = handle_prepare_edit_context(state, params);
    serde_json::from_str(&out).unwrap()
}

fn apply(state: &SynrepoState, params: serde_json::Value) -> serde_json::Value {
    let params = serde_json::from_value(params).unwrap();
    let out = handle_apply_anchor_edits(state, params);
    serde_json::from_str(&out).unwrap()
}

#[test]
fn prepare_file_returns_deterministic_anchors_and_new_versions() {
    let (_dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\nthree\n")]);
    let first = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-a" }),
    );
    let second = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-a" }),
    );

    assert_eq!(first["anchors"][0]["anchor"], "L000001");
    assert_eq!(first["anchors"][1]["anchor"], "L000002");
    assert_ne!(
        first["anchor_state_version"], second["anchor_state_version"],
        "each prepare should advance the opaque anchor state version"
    );
    assert_eq!(first["path"], "src/lib.rs");
    assert!(first["file_id"].as_str().is_some());
    assert!(first["content_hash"].as_str().is_some());
    assert!(first["source_hash"].as_str().is_some());
}

#[test]
fn prepare_symbol_range_and_budgeted_output() {
    let (_dir, state) = state_with_files(&[(
        "src/lib.rs",
        "fn helper() -> i32 {\n    1\n}\n\nfn other() {}\n",
    )]);
    let symbol = prepare(
        &state,
        json!({ "target": "helper", "target_kind": "symbol", "budget_lines": 2 }),
    );
    assert_eq!(symbol["range"]["start_line"], 1);
    assert_eq!(symbol["range"]["end_line"], 2);
    assert_eq!(symbol["range"]["truncated"], true);
    assert!(symbol["symbol_id"].as_str().is_some());

    let range = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "range", "start_line": 5, "end_line": 5 }),
    );
    assert_eq!(range["anchors"][0]["line"], 5);
}

#[test]
fn apply_replace_insert_and_delete() {
    let (dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\nthree\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-edit" }),
    );
    let result = apply(
        &state,
        json!({
            "edits": [
                {
                    "task_id": "task-edit",
                    "anchor_state_version": prepared["anchor_state_version"],
                    "path": "src/lib.rs",
                    "content_hash": prepared["content_hash"],
                    "anchor": "L000001",
                    "edit_type": "insert_after",
                    "text": "one-point-five"
                },
                {
                    "task_id": "task-edit",
                    "anchor_state_version": prepared["anchor_state_version"],
                    "path": "src/lib.rs",
                    "content_hash": prepared["content_hash"],
                    "anchor": "L000002",
                    "edit_type": "replace",
                    "text": "TWO"
                },
                {
                    "task_id": "task-edit",
                    "anchor_state_version": prepared["anchor_state_version"],
                    "path": "src/lib.rs",
                    "content_hash": prepared["content_hash"],
                    "anchor": "L000003",
                    "edit_type": "delete"
                }
            ]
        }),
    );

    assert_eq!(result["status"], "completed", "{result}");
    let content = fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
    assert_eq!(content, "one\none-point-five\nTWO\n");
    assert!(
        !dir.path().join("src/lib.rs.synrepo-edit-tmp").exists(),
        "anchored edit should not leave the old deterministic temp path behind"
    );
    assert_eq!(result["atomicity"]["cross_file"], true);
    assert_eq!(result["diagnostics"]["command_execution"], "unavailable");
}

#[test]
fn stale_content_hash_and_missing_session_reject_without_writing() {
    let (dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-stale" }),
    );
    fs::write(dir.path().join("src/lib.rs"), "one\nchanged\n").unwrap();
    let stale = apply(
        &state,
        json!({ "edits": [{
            "task_id": "task-stale",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": "src/lib.rs",
            "content_hash": prepared["content_hash"],
            "anchor": "L000002",
            "edit_type": "replace",
            "text": "TWO"
        }] }),
    );
    assert_eq!(stale["files"][0]["status"], "rejected");
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "one\nchanged\n"
    );

    let missing = apply(
        &state,
        json!({ "edits": [{
            "task_id": "missing",
            "anchor_state_version": "asv-missing",
            "path": "src/lib.rs",
            "content_hash": prepared["content_hash"],
            "anchor": "L000001",
            "edit_type": "replace",
            "text": "ONE"
        }] }),
    );
    assert_eq!(missing["files"][0]["status"], "rejected");
}

#[test]
fn overlapping_edit_rejects_file_atomically() {
    let (dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\nthree\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-overlap" }),
    );
    let result = apply(
        &state,
        json!({ "edits": [
            {
                "task_id": "task-overlap",
                "anchor_state_version": prepared["anchor_state_version"],
                "path": "src/lib.rs",
                "content_hash": prepared["content_hash"],
                "anchor": "L000001",
                "end_anchor": "L000002",
                "edit_type": "replace",
                "text": "ONE-TWO"
            },
            {
                "task_id": "task-overlap",
                "anchor_state_version": prepared["anchor_state_version"],
                "path": "src/lib.rs",
                "content_hash": prepared["content_hash"],
                "anchor": "L000002",
                "edit_type": "replace",
                "text": "TWO"
            }
        ] }),
    );
    assert_eq!(result["files"][0]["status"], "rejected", "{result}");
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "one\ntwo\nthree\n"
    );
}

#[test]
fn duplicate_line_text_uses_specific_anchor_without_ambiguity() {
    let (dir, state) = state_with_files(&[("src/lib.rs", "same\nsame\nsame\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-dup" }),
    );
    let result = apply(
        &state,
        json!({ "edits": [{
            "task_id": "task-dup",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": "src/lib.rs",
            "content_hash": prepared["content_hash"],
            "anchor": "L000002",
            "edit_type": "replace",
            "text": "middle"
        }] }),
    );
    assert_eq!(result["files"][0]["status"], "applied", "{result}");
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "same\nmiddle\nsame\n"
    );
}

#[test]
fn multi_file_request_preflights_all_files_before_writing() {
    let (dir, state) = state_with_files(&[("src/a.rs", "a1\na2\n"), ("src/b.rs", "b1\nb2\n")]);
    let a = prepare(
        &state,
        json!({ "target": "src/a.rs", "target_kind": "file", "task_id": "task-a" }),
    );
    let b = prepare(
        &state,
        json!({ "target": "src/b.rs", "target_kind": "file", "task_id": "task-b" }),
    );
    fs::write(dir.path().join("src/b.rs"), "b1\nchanged\n").unwrap();

    let result = apply(
        &state,
        json!({ "edits": [
            {
                "task_id": "task-a",
                "anchor_state_version": a["anchor_state_version"],
                "path": "src/a.rs",
                "content_hash": a["content_hash"],
                "anchor": "L000002",
                "edit_type": "replace",
                "text": "A2"
            },
            {
                "task_id": "task-b",
                "anchor_state_version": b["anchor_state_version"],
                "path": "src/b.rs",
                "content_hash": b["content_hash"],
                "anchor": "L000002",
                "edit_type": "replace",
                "text": "B2"
            }
        ] }),
    );
    assert_eq!(result["status"], "rejected", "{result}");
    assert_eq!(result["files"][0]["status"], "rejected", "{result}");
    assert_eq!(result["files"][1]["status"], "not_applied", "{result}");
    assert_eq!(result["atomicity"]["cross_file"], true);
    assert_eq!(
        fs::read_to_string(dir.path().join("src/a.rs")).unwrap(),
        "a1\na2\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src/b.rs")).unwrap(),
        "b1\nchanged\n"
    );
    let metrics = context_metrics::load(&Config::synrepo_dir(dir.path())).unwrap();
    assert_eq!(metrics.anchored_edit_accepted_total, 0);
    assert_eq!(metrics.anchored_edit_rejected_total, 2);
}

#[cfg(unix)]
#[test]
fn writer_lock_conflict_rejects_without_writing() {
    let (dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-lock" }),
    );
    let synrepo_dir = Config::synrepo_dir(dir.path());
    let ownership = WriterOwnership {
        pid: 424_242,
        acquired_at: "test".to_string(),
    };
    let _holder = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let result = apply(
        &state,
        json!({ "edits": [{
            "task_id": "task-lock",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": "src/lib.rs",
            "content_hash": prepared["content_hash"],
            "anchor": "L000002",
            "edit_type": "replace",
            "text": "TWO"
        }] }),
    );

    assert_eq!(result["status"], "writer_lock_conflict", "{result}");
    assert_eq!(result["writer_lock"]["holder_pid"], 424_242);
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "one\ntwo\n"
    );
}

#[test]
fn prepare_not_found_returns_error() {
    let (_dir, state) = state_with_files(&[("src/lib.rs", "one\n")]);
    let result = prepare(&state, json!({ "target": "missing_symbol" }));
    assert_eq!(result["error"]["code"], "NOT_FOUND");
    assert!(result["error_message"]
        .as_str()
        .unwrap()
        .contains("target not found"));
}
