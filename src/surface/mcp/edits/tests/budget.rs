use std::fs;

use serde_json::json;

use super::{apply, prepare, state_with_files};

#[test]
fn apply_enforces_text_line_budget() {
    let (dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-budget" }),
    );
    let edit = |text: &str| {
        json!({
            "task_id": "task-budget",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": "src/lib.rs",
            "content_hash": prepared["content_hash"],
            "anchor": "L000001",
            "edit_type": "insert_after",
            "text": text,
        })
    };

    let over_budget = apply(
        &state,
        json!({ "max_lines": 1, "edits": [edit("alpha\nbeta")] }),
    );
    assert!(over_budget["error_message"]
        .as_str()
        .unwrap()
        .contains("exceeding max_lines"));
    let hard_ceiling = apply(
        &state,
        json!({ "max_lines": 5001, "edits": [edit("alpha")] }),
    );
    assert!(hard_ceiling["error_message"]
        .as_str()
        .unwrap()
        .contains("between 1 and 5000"));
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "one\ntwo\n"
    );
    let ok = apply(
        &state,
        json!({ "max_lines": 2, "edits": [edit("alpha\nbeta")] }),
    );
    assert_eq!(ok["status"], "completed", "{ok}");
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "one\nalpha\nbeta\ntwo\n"
    );
}

#[test]
fn apply_enforces_payload_caps_before_path_resolution() {
    let (_dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-caps" }),
    );
    let edit = |path: &str, text: &str| {
        json!({
            "task_id": "task-caps",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": path,
            "content_hash": prepared["content_hash"],
            "anchor": "L000001",
            "edit_type": "insert_after",
            "text": text,
        })
    };

    let too_many_edits = (0..101)
        .map(|_| edit("src/lib.rs", "alpha"))
        .collect::<Vec<_>>();
    let result = apply(&state, json!({ "edits": too_many_edits }));
    assert_eq!(result["ok"], false);
    assert!(result["error_message"]
        .as_str()
        .unwrap()
        .contains("exceeding hard limit 100"));

    let too_many_files = (0..21)
        .map(|i| edit(&format!("src/{i}.rs"), "alpha"))
        .collect::<Vec<_>>();
    let result = apply(&state, json!({ "edits": too_many_files }));
    assert_eq!(result["ok"], false);
    assert!(result["error_message"]
        .as_str()
        .unwrap()
        .contains("distinct files"));

    let long_line = "x".repeat(256 * 1024);
    let result = apply(&state, json!({ "edits": [edit("src/lib.rs", &long_line)] }));
    assert_eq!(result["ok"], false);
    assert!(result["error_message"]
        .as_str()
        .unwrap()
        .contains("single edit payload"));
}
