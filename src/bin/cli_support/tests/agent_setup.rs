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

    let tool = AgentTool::Generic;
    let out_path = tool.output_path(repo);
    fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    // Invalid UTF-8: lone 0x80 continuation byte and bare 0xFF.
    fs::write(&out_path, [0x80u8, 0xffu8, 0xfeu8, 0x80u8]).unwrap();

    agent_setup(repo, tool, false, true).expect("regen must succeed");

    let after = fs::read_to_string(&out_path).expect("file must be valid UTF-8 after regen");
    assert_eq!(
        after,
        tool.shim_content(),
        "regen must overwrite non-UTF8 existing content with the canonical shim"
    );
}

/// Local fallback shims must replace a symlink at the target path instead of
/// writing through it.
#[cfg(unix)]
#[test]
fn agent_setup_regen_replaces_symlink_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let repo = dir.path();

    let tool = AgentTool::Generic;
    let out_path = tool.output_path(repo);
    let parent = out_path.parent().unwrap();
    fs::create_dir_all(parent).unwrap();

    let sibling = parent.join("elsewhere.txt");
    fs::write(&sibling, "sibling original").unwrap();
    symlink(&sibling, &out_path).unwrap();

    agent_setup(repo, tool, false, true).expect("regen must succeed");

    let link_meta = fs::symlink_metadata(&out_path).unwrap();
    assert!(
        !link_meta.file_type().is_symlink(),
        "shim target must be replaced by a regular file"
    );

    let shim_content = fs::read_to_string(&out_path).unwrap();
    assert_eq!(shim_content, tool.shim_content());

    let sibling_content = fs::read_to_string(&sibling).unwrap();
    assert_eq!(
        sibling_content, "sibling original",
        "write must not follow the symlink to its target"
    );
}

#[cfg(unix)]
#[test]
fn agent_setup_rejects_symlinked_parent_directory() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let repo = dir.path();

    let tool = AgentTool::Goose;
    symlink(outside.path(), repo.join(".goose")).unwrap();

    let err = agent_setup(repo, tool, true, false)
        .expect_err("agent setup must reject symlinked parent directories");

    assert!(
        err.to_string()
            .contains("refusing to write shim through symlink"),
        "unexpected error: {err:#}"
    );
    assert!(
        fs::read_dir(outside.path()).unwrap().next().is_none(),
        "outside symlink target must remain untouched"
    );
}

/// If the path that would be the shim's parent directory exists as a regular
/// file, `fs::create_dir_all` fails and `agent_setup` propagates an error
/// that names the parent path. Guards the error branch in `basic.rs`.
#[test]
fn agent_setup_returns_error_when_parent_is_a_file() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    let tool = AgentTool::Goose;
    let out_path = tool.output_path(repo);
    let parent = out_path.parent().unwrap();
    // Create intermediate dirs up to the parent, then plant a regular file
    // at the parent path so `create_dir_all(parent)` fails.
    if let Some(grandparent) = parent.parent() {
        fs::create_dir_all(grandparent).unwrap();
    }
    fs::write(parent, "not a directory").unwrap();

    let err = agent_setup(repo, tool, false, false)
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

    let tool = AgentTool::Generic;
    let out_path = tool.output_path(repo);
    fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    let garbage = "not-the-shim: arbitrary pre-existing content\n";
    fs::write(&out_path, garbage).unwrap();

    agent_setup(repo, tool, false, false)
        .expect("must return Ok even when skipping an existing file");

    let after = fs::read_to_string(&out_path).unwrap();
    assert_eq!(
        after, garbage,
        "existing file must be preserved when neither --force nor --regen is set"
    );
}
