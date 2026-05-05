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
