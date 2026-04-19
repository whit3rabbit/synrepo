//! CLI-level tests for `synrepo agent-setup` edge cases around malformed
//! existing shim files. The current implementation uses
//! `std::fs::read_to_string(...).unwrap_or_default()` in the `--regen` path,
//! silently treating non-UTF8 content as empty. These tests pin the
//! resulting behavior so future refactors do not quietly change it.

use std::fs;
use tempfile::tempdir;

use crate::agent_setup;
use crate::cli_support::agent_shims::AgentTool;

/// Non-UTF8 bytes at the shim target are treated as empty content by the
/// `--regen` branch, so regen sees a difference and overwrites. Pin that
/// outcome: the shim file ends up with the canonical text.
#[test]
fn agent_setup_regen_overwrites_non_utf8_existing() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    let out_path = AgentTool::Claude.output_path(repo);
    fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    // Invalid UTF-8: lone 0x80 continuation byte and bare 0xFF.
    fs::write(&out_path, [0x80u8, 0xffu8, 0xfeu8, 0x80u8]).unwrap();

    agent_setup(repo, AgentTool::Claude, false, true).expect("regen must succeed");

    let after = fs::read_to_string(&out_path).expect("file must be valid UTF-8 after regen");
    assert_eq!(
        after,
        AgentTool::Claude.shim_content(),
        "regen must overwrite non-UTF8 existing content with the canonical shim"
    );
}

/// `std::fs::write` follows symlinks: writing to a symlink overwrites the
/// file it points to, leaving the symlink intact. This test pins that
/// behavior so a future switch to `O_NOFOLLOW` (or any path that replaces
/// the symlink with a regular file) forces an explicit decision.
#[cfg(unix)]
#[test]
fn agent_setup_regen_follows_symlink_to_sibling() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let repo = dir.path();

    let out_path = AgentTool::Claude.output_path(repo);
    let parent = out_path.parent().unwrap();
    fs::create_dir_all(parent).unwrap();

    let sibling = parent.join("elsewhere.txt");
    fs::write(&sibling, "sibling original").unwrap();
    symlink(&sibling, &out_path).unwrap();

    agent_setup(repo, AgentTool::Claude, false, true).expect("regen must succeed");

    // Symlink is preserved (not replaced by a regular file).
    let link_meta = fs::symlink_metadata(&out_path).unwrap();
    assert!(
        link_meta.file_type().is_symlink(),
        "shim target must remain a symlink"
    );

    // Write went through to the sibling.
    let sibling_content = fs::read_to_string(&sibling).unwrap();
    assert_eq!(
        sibling_content,
        AgentTool::Claude.shim_content(),
        "write followed the symlink; sibling now holds the shim content"
    );
}

/// If the path that would be the shim's parent directory exists as a regular
/// file, `fs::create_dir_all` fails and `agent_setup` propagates an error
/// that names the parent path. Guards the error branch in `basic.rs`.
#[test]
fn agent_setup_returns_error_when_parent_is_a_file() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    let out_path = AgentTool::Claude.output_path(repo);
    let parent = out_path.parent().unwrap();
    // Create intermediate dirs up to the parent, then plant a regular file
    // at the parent path so `create_dir_all(parent)` fails.
    if let Some(grandparent) = parent.parent() {
        fs::create_dir_all(grandparent).unwrap();
    }
    fs::write(parent, "not a directory").unwrap();

    let err = agent_setup(repo, AgentTool::Claude, false, false)
        .expect_err("must error when parent path is a regular file");
    let msg = err.to_string();
    assert!(
        msg.contains(&parent.display().to_string()),
        "error must name the blocked parent path; got: {msg}"
    );

    // The blocking file must remain untouched.
    let content = fs::read_to_string(parent).unwrap();
    assert_eq!(content, "not a directory");
}

/// Without `--force` and without `--regen`, an existing shim file is left
/// alone (even if its content is malformed). The command returns Ok and
/// prints a hint; this pins that it does not silently overwrite.
#[test]
fn agent_setup_without_force_does_not_overwrite_malformed_existing() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    let out_path = AgentTool::Claude.output_path(repo);
    fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    let garbage = "not-the-shim: arbitrary pre-existing content\n";
    fs::write(&out_path, garbage).unwrap();

    agent_setup(repo, AgentTool::Claude, false, false)
        .expect("must return Ok even when skipping an existing file");

    let after = fs::read_to_string(&out_path).unwrap();
    assert_eq!(
        after, garbage,
        "existing file must be preserved when neither --force nor --regen is set"
    );
}
