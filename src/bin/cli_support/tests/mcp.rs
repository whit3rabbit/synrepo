//! Readiness-gate tests for `synrepo mcp`. These exist to catch regressions
//! where the server would happily accept clients against an unready store and
//! only surface the failure per tool call.

use std::fs;

use tempfile::tempdir;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::store::compatibility::snapshot_path;

use super::support::git;
use crate::prepare_mcp_state;

fn setup_bootstrapped_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    git(&dir, &["init", "-b", "main"]);
    git(&dir, &["config", "user.email", "test@example.com"]);
    git(&dir, &["config", "user.name", "Test"]);
    fs::write(repo.join("lib.rs"), "fn main() {}").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "init"]);

    bootstrap(repo, None).unwrap();

    let repo_path = repo.to_path_buf();
    (dir, repo_path)
}

#[test]
fn prepare_state_succeeds_on_fresh_bootstrap() {
    let (dir, repo) = setup_bootstrapped_repo();
    prepare_mcp_state(&repo).expect("fresh bootstrap must pass the MCP readiness gate");
    drop(dir);
}

#[test]
fn prepare_state_fails_when_compatibility_snapshot_is_missing() {
    let (dir, repo) = setup_bootstrapped_repo();
    let synrepo_dir = Config::synrepo_dir(&repo);

    // Same trick upgrade's own tests use: a materialized canonical store with
    // no compatibility snapshot is a Block action, which must fail the gate
    // instead of letting MCP come up and surface the error per tool call.
    let snap = snapshot_path(&synrepo_dir);
    fs::remove_file(&snap).expect("snapshot must exist after bootstrap");

    let err = match prepare_mcp_state(&repo) {
        Err(e) => e,
        Ok(_) => panic!("blocking compatibility must fail the gate"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("synrepo upgrade"),
        "fail-fast message must point users at `synrepo upgrade`, got: {msg}"
    );
    drop(dir);
}

#[test]
fn prepare_state_fails_on_uninitialized_repo() {
    let dir = tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    // Deliberately no `synrepo init` / `bootstrap` — the server must refuse
    // to start rather than serving a tool that then trips over a missing
    // config.
    let err = match prepare_mcp_state(&repo) {
        Err(e) => e,
        Ok(_) => panic!("uninitialized repo must fail the gate"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("synrepo init"),
        "fail-fast message must point users at `synrepo init`, got: {msg}"
    );
    drop(dir);
}
