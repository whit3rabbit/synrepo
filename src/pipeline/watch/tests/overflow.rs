use std::{fs, time::Duration};

use crate::{pipeline::watch::ReconcileOutcome, store::sqlite::SqliteGraphStore};

use super::{setup_test_repo, wait_for};

#[test]
fn watch_path_overflow_runs_full_reconcile() {
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let paths = [
        repo.join("src/overflow_one.rs"),
        repo.join("src/overflow_two.rs"),
        repo.join("src/overflow_three.rs"),
    ];

    fs::write(&paths[0], "pub fn overflow_one() {}\n").unwrap();
    fs::write(&paths[1], "pub fn overflow_two() {}\n").unwrap();
    fs::write(&paths[2], "pub fn overflow_three() {}\n").unwrap();

    let mut pending = super::super::pending::PendingWatchChanges::default();
    pending.record(3, paths.to_vec(), 2);
    let batch = pending.take(2);
    assert!(batch.force_full_reconcile);

    let touched = if batch.force_full_reconcile {
        None
    } else {
        Some(batch.touched_paths.as_slice())
    };
    let attempt = super::super::reconcile::run_reconcile_attempt_with_touched_paths(
        &repo,
        &config,
        &synrepo_dir,
        touched,
        true,
    );
    assert!(matches!(attempt.outcome, ReconcileOutcome::Completed(_)));

    wait_for(
        || graph_contains_path(&synrepo_dir, "src/overflow_three.rs"),
        Duration::from_secs(5),
    );
}

fn graph_contains_path(synrepo_dir: &std::path::Path, relative_path: &str) -> bool {
    let Ok(graph) = SqliteGraphStore::open(&synrepo_dir.join("graph")) else {
        return false;
    };
    graph
        .all_file_paths()
        .map(|paths| paths.iter().any(|(path, _)| path == relative_path))
        .unwrap_or(false)
}
