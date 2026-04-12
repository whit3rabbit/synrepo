use std::path::Path;

use crate::{
    config::Config,
    pipeline::{
        structural::CompileSummary,
        watch::{persist_reconcile_state, ReconcileOutcome},
        writer::{writer_lock_path, WriterOwnership},
    },
    store::compatibility::{ensure_runtime_layout, write_runtime_snapshot},
};

pub(super) fn init_synrepo(synrepo_dir: &Path) {
    ensure_runtime_layout(synrepo_dir).unwrap();
    write_runtime_snapshot(synrepo_dir, &Config::default()).unwrap();
}

pub(super) fn init_synrepo_with_completed_reconcile(synrepo_dir: &Path) {
    init_synrepo(synrepo_dir);
    let summary = CompileSummary {
        files_discovered: 1,
        symbols_extracted: 1,
        ..Default::default()
    };
    persist_reconcile_state(synrepo_dir, &ReconcileOutcome::Completed(summary), 0);
}

pub(super) fn setup_repo_for_sync(
    dir: &tempfile::TempDir,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let repo = dir.path().to_path_buf();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
    let synrepo_dir = repo.join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);
    (repo, synrepo_dir)
}

pub(super) fn write_foreign_lock(synrepo_dir: &Path) {
    let ownership = WriterOwnership {
        pid: 42,
        acquired_at: "2026-01-01T00:00:00Z".to_string(),
    };
    std::fs::write(
        writer_lock_path(synrepo_dir),
        serde_json::to_string_pretty(&ownership).unwrap(),
    )
    .unwrap();
}
