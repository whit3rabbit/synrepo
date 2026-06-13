use std::fs;

use fs2::FileExt;
use serde_json::json;
use synrepo::surface::card::accounting::estimate_tokens_bytes;

use super::*;

fn observation(path: &str, size_bytes: u64, modified_unix_secs: u64) -> ReadObservation {
    observation_with_nanos(
        path,
        size_bytes,
        modified_unix_secs,
        modified_unix_secs.saturating_mul(1_000_000_000),
    )
}

fn observation_with_nanos(
    path: &str,
    size_bytes: u64,
    modified_unix_secs: u64,
    modified_unix_nanos: u64,
) -> ReadObservation {
    ReadObservation {
        rel_path: path.to_string(),
        metadata: ReadMetadata {
            size_bytes,
            modified_unix_secs,
            modified_unix_nanos,
            estimated_tokens: estimate_tokens_bytes(size_bytes as usize),
        },
    }
}

#[test]
fn first_read_records_cost_then_repeated_read_warns() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");

    let first = update_state(&synrepo_dir, observation("src/lib.rs", 300, 1), 10)
        .unwrap()
        .unwrap();
    let second = update_state(&synrepo_dir, observation("src/lib.rs", 300, 1), 11)
        .unwrap()
        .unwrap();

    assert!(!first.repeated);
    assert_eq!(first.estimated_tokens, 100);
    assert!(second.repeated);
}

#[test]
fn changed_file_resets_without_repeat_warning() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");

    update_state(&synrepo_dir, observation("src/lib.rs", 300, 1), 10)
        .unwrap()
        .unwrap();
    let changed = update_state(&synrepo_dir, observation("src/lib.rs", 301, 2), 11)
        .unwrap()
        .unwrap();

    assert!(!changed.repeated);
}

#[test]
fn same_size_same_second_edit_resets_without_repeat_warning() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");

    update_state(
        &synrepo_dir,
        observation_with_nanos("src/lib.rs", 300, 1, 1_000_000_001),
        10,
    )
    .unwrap()
    .unwrap();
    let changed = update_state(
        &synrepo_dir,
        observation_with_nanos("src/lib.rs", 300, 1, 1_000_000_002),
        11,
    )
    .unwrap()
    .unwrap();

    assert!(!changed.repeated);
}

#[test]
fn expired_entries_do_not_count_as_repeats() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");

    update_state(&synrepo_dir, observation("src/lib.rs", 300, 1), 10)
        .unwrap()
        .unwrap();
    let late = update_state(
        &synrepo_dir,
        observation("src/lib.rs", 300, 1),
        10 + TTL_SECS + 1,
    )
    .unwrap()
    .unwrap();

    assert!(!late.repeated);
}

#[test]
fn caps_state_entries() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");

    for i in 0..(MAX_ENTRIES + 4) {
        update_state(
            &synrepo_dir,
            observation(&format!("src/file_{i}.rs"), 30, i as u64),
            i as u64,
        )
        .unwrap()
        .unwrap();
    }

    let state = read_state(&synrepo_dir.join("state").join(STATE_FILE)).unwrap();
    assert_eq!(state.entries.len(), MAX_ENTRIES);
    assert!(state
        .entries
        .iter()
        .all(|entry| entry.path != "src/file_0.rs"));
}

#[test]
fn corrupt_state_fails_open() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir).unwrap();
    fs::write(state_dir.join(STATE_FILE), b"not json").unwrap();

    let result = update_state(&synrepo_dir, observation("src/lib.rs", 300, 1), 10);

    assert!(result.is_err());
}

#[test]
fn corrupt_state_suppresses_hook_hint() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::create_dir_all(&state_dir).unwrap();
    fs::write(repo.path().join("src/lib.rs"), b"fn main() {}\n").unwrap();
    fs::write(state_dir.join(STATE_FILE), b"not json").unwrap();
    let input = json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "src/lib.rs" }
    });

    let hint = read_hint_best_effort(
        HookClient::Claude,
        HookEvent::PreToolUse,
        &input,
        &synrepo_dir,
    );

    assert!(hint.is_none());
}

#[test]
fn locked_state_fails_open() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(state_dir.join(LOCK_FILE))
        .unwrap();
    lock.lock_exclusive().unwrap();

    let result = update_state(&synrepo_dir, observation("src/lib.rs", 300, 1), 10).unwrap();

    assert!(result.is_none());
}

#[test]
fn observe_path_ignores_outside_repo_and_synrepo_state() {
    let repo = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::create_dir_all(repo.path().join(".synrepo/state")).unwrap();
    fs::write(repo.path().join(".synrepo/state/data.json"), b"{}").unwrap();

    assert!(observe_path(repo.path(), outside.path().to_str().unwrap())
        .unwrap()
        .is_none());
    assert!(observe_path(repo.path(), ".synrepo/state/data.json")
        .unwrap()
        .is_none());
}

#[test]
fn codex_reader_parser_is_conservative() {
    assert_eq!(
        extract_direct_reader_path("rtk proxy cat src/lib.rs"),
        Some("src/lib.rs".to_string())
    );
    assert_eq!(
        extract_direct_reader_path("sed -n '1,20p' src/lib.rs"),
        Some("src/lib.rs".to_string())
    );
    assert!(extract_direct_reader_path("cat src/lib.rs | wc -l").is_none());
    assert!(extract_direct_reader_path("cat src/*.rs").is_none());
    assert!(extract_direct_reader_path("cat src/a.rs src/b.rs").is_none());
}

#[test]
fn sed_in_place_edits_are_not_reads() {
    assert!(extract_direct_reader_path("sed -i 's/a/b/' src/lib.rs").is_none());
    assert!(extract_direct_reader_path("sed -i.bak 's/a/b/' src/lib.rs").is_none());
    assert!(extract_direct_reader_path("sed --in-place 's/a/b/' src/lib.rs").is_none());
}
