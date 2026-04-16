//! CLI smoke tests for the compact command.

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::compact::{execute_compact, plan_compact};
use synrepo::pipeline::maintenance::CompactPolicy;

/// Verify compact dry run prints plan without mutating via library API.
#[test]
fn compact_dry_run_library() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    // Initialize a repo.
    bootstrap(repo, None).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo);
    let config = Config::load(repo).unwrap();

    // Plan without applying.
    let plan = plan_compact(&synrepo_dir, &config, CompactPolicy::Default).unwrap();

    // Should return a plan.
    assert!(!plan.actions.is_empty());

    // No state file created yet.
    let state_file = synrepo_dir.join("state/compact-state.json");
    assert!(!state_file.exists(), "dry run must not create state file");
}

/// Verify compact --apply via library API.
#[test]
fn compact_apply_library() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    // Initialize a repo.
    bootstrap(repo, None).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo);
    let config = Config::load(repo).unwrap();

    // Plan and execute.
    let plan = plan_compact(&synrepo_dir, &config, CompactPolicy::Default).unwrap();
    let summary = execute_compact(&synrepo_dir, &plan, CompactPolicy::Default).unwrap();

    // Should produce summary.
    assert!(summary.wal_checkpoint_completed);

    // State file created.
    let state_file = synrepo_dir.join("state/compact-state.json");
    assert!(state_file.exists(), "apply must create state file");
}
