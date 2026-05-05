use std::fs;

use tempfile::tempdir;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::explain::accounting;
#[cfg(unix)]
use synrepo::pipeline::writer::{
    hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
};

use crate::prepare_mcp_state;

fn setup_bootstrapped_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    fs::write(repo.join("lib.rs"), "fn main() {}\n").unwrap();
    bootstrap(&repo, None, false).unwrap();
    (dir, repo)
}

#[cfg(unix)]
#[test]
fn refresh_commentary_rejects_when_writer_lock_is_held() {
    let (dir, repo) = setup_bootstrapped_repo();
    let state = prepare_mcp_state(&repo).expect("MCP state should load");
    let synrepo_dir = Config::synrepo_dir(&repo);
    let ownership = WriterOwnership {
        pid: 424_242,
        acquired_at: "test".to_string(),
    };
    let _holder = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let output =
        synrepo::surface::mcp::cards::handle_refresh_commentary(&state, "main".to_string());
    let json: serde_json::Value =
        serde_json::from_str(&output).expect("refresh_commentary should return JSON");
    let error = json["error_message"]
        .as_str()
        .expect("lock conflict should error");

    assert_eq!(json["error"]["code"], "LOCKED", "{output}");
    assert!(
        error.contains("writer lock held by pid 424242"),
        "expected writer lock conflict, got: {output}"
    );
    assert!(
        !accounting::log_path(&synrepo_dir).exists(),
        "refresh must not record explain accounting while lock is held"
    );
    drop(dir);
}
