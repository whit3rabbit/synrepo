use std::{fs, os::unix::fs::symlink};

use serde_json::json;
use tempfile::tempdir;

use super::{apply, prepare, state_with_files};
use crate::{bootstrap, config::Config, surface::mcp::SynrepoState};

#[test]
fn prepare_rejects_symlink_escape_before_reading() {
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("secret.rs"), "pub fn secret() {}\n").unwrap();
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    symlink(
        outside.path().join("secret.rs"),
        dir.path().join("src/escape.rs"),
    )
    .unwrap();
    bootstrap::bootstrap(dir.path(), None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(dir.path()).unwrap(),
        repo_root: dir.path().to_path_buf(),
    };

    let result = prepare(
        &state,
        json!({ "target": "src/escape.rs", "target_kind": "file", "task_id": "task-escape" }),
    );
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|err| err.contains("outside repo root")),
        "{result}"
    );
}

#[test]
fn apply_rejects_symlink_escape_after_prepare() {
    let outside = tempdir().unwrap();
    let outside_file = outside.path().join("secret.rs");
    fs::write(&outside_file, "outside\n").unwrap();
    let (dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\n")]);
    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-swap" }),
    );
    let in_repo = dir.path().join("src/lib.rs");
    fs::remove_file(&in_repo).unwrap();
    symlink(&outside_file, &in_repo).unwrap();

    let result = apply(
        &state,
        json!({ "edits": [{
            "task_id": "task-swap",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": "src/lib.rs",
            "content_hash": prepared["content_hash"],
            "anchor": "L000001",
            "edit_type": "replace",
            "text": "ONE"
        }] }),
    );
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|err| err.contains("outside repo root")),
        "{result}"
    );
    assert_eq!(fs::read_to_string(&outside_file).unwrap(), "outside\n");
}
