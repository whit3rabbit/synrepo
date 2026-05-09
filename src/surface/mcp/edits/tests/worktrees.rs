use std::{fs, path::Path, process::Command};

use serde_json::json;
use tempfile::tempdir;

use super::{apply, prepare};
use crate::{
    bootstrap::bootstrap,
    config::Config,
    substrate::{discover, DiscoveryRootKind},
    surface::mcp::SynrepoState,
};

#[test]
fn anchored_edits_respect_requested_worktree_root() {
    let fixture = worktree_fixture();

    let primary = prepare(
        &fixture.state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-primary" }),
    );
    assert_eq!(primary["root_id"], "primary", "{primary}");
    assert!(primary["context"].as_str().unwrap().contains("primary"));

    let prepared = prepare(
        &fixture.state,
        json!({
            "target": "src/lib.rs",
            "target_kind": "file",
            "root_id": fixture.worktree_root_id,
            "task_id": "task-worktree"
        }),
    );
    assert_eq!(prepared["root_id"], fixture.worktree_root_id, "{prepared}");
    assert_eq!(prepared["is_primary_root"], false, "{prepared}");
    assert!(prepared["context"].as_str().unwrap().contains("worktree"));

    let result = apply(
        &fixture.state,
        json!({ "edits": [{
            "task_id": "task-worktree",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": "src/lib.rs",
            "root_id": prepared["root_id"],
            "content_hash": prepared["content_hash"],
            "anchor": "L000001",
            "edit_type": "replace",
            "text": "pub fn worktree_changed() {}"
        }] }),
    );
    assert_eq!(result["status"], "completed", "{result}");
    assert_eq!(
        fs::read_to_string(fixture.main.path().join("src/lib.rs")).unwrap(),
        "pub fn primary() {}\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.worktree.path().join("src/lib.rs")).unwrap(),
        "pub fn worktree_changed() {}\n"
    );

    let unknown = prepare(
        &fixture.state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "root_id": "missing-root" }),
    );
    assert_eq!(unknown["error"]["code"], "INVALID_PARAMETER", "{unknown}");

    let mismatched = prepare(
        &fixture.state,
        json!({ "target": primary["file_id"], "root_id": fixture.worktree_root_id }),
    );
    assert_eq!(
        mismatched["error"]["code"], "INVALID_PARAMETER",
        "{mismatched}"
    );
}

struct WorktreeFixture {
    main: tempfile::TempDir,
    worktree: tempfile::TempDir,
    worktree_root_id: String,
    state: SynrepoState,
}

fn worktree_fixture() -> WorktreeFixture {
    let main = tempdir().unwrap();
    let worktree = tempdir().unwrap();
    fs::create_dir_all(main.path().join("src")).unwrap();
    fs::write(main.path().join("src/lib.rs"), "pub fn primary() {}\n").unwrap();

    git(main.path(), &["init"]);
    git(main.path(), &["config", "user.email", "test@example.com"]);
    git(main.path(), &["config", "user.name", "Test User"]);
    git(main.path(), &["add", "."]);
    git(main.path(), &["commit", "-m", "initial"]);
    fs::remove_dir(worktree.path()).unwrap();
    git(
        main.path(),
        &[
            "worktree",
            "add",
            "-b",
            "wt-branch",
            path_str(worktree.path()),
        ],
    );
    fs::write(worktree.path().join("src/lib.rs"), "pub fn worktree() {}\n").unwrap();

    bootstrap(main.path(), None, false).unwrap();
    let config = Config::load(main.path()).unwrap();
    let worktree_root_id = discover(main.path(), &config)
        .unwrap()
        .into_iter()
        .find(|file| {
            file.root_kind == DiscoveryRootKind::Worktree && file.relative_path == "src/lib.rs"
        })
        .unwrap()
        .root_discriminant;
    let state = SynrepoState {
        config,
        repo_root: main.path().to_path_buf(),
    };
    WorktreeFixture {
        main,
        worktree,
        worktree_root_id,
        state,
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}
