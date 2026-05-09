use std::{fs, path::Path, process::Command};

use serde_json::Value;
use tempfile::tempdir;

use crate::{
    bootstrap::bootstrap,
    config::Config,
    surface::mcp::{
        compact::OutputMode,
        search::{handle_search, SearchMode, SearchParams},
        SynrepoState,
    },
};

#[test]
fn search_returns_worktree_only_matches_with_root_metadata() {
    let fixture = worktree_fixture();
    let worktree_root = worktree_root_id(&fixture.state);

    let raw = search_json(
        &fixture.state,
        params("wt_unique_token", OutputMode::Default),
    );
    let rows = raw["results"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "{raw}");
    for row in rows {
        assert_eq!(row["path"], "src/wt_only.rs");
        assert_eq!(row["root_id"], worktree_root);
        assert_eq!(row["is_primary_root"], false);
        assert!(row["file_id"].as_str().is_some(), "{raw}");
    }

    let compact = search_json(
        &fixture.state,
        params("wt_unique_token", OutputMode::Compact),
    );
    let groups = compact["file_groups"].as_array().unwrap();
    assert_eq!(groups[0]["root_id"], worktree_root, "{compact}");
    assert_eq!(groups[0]["is_primary_root"], false, "{compact}");
    assert_eq!(
        compact["suggested_card_requests"][0]["root_id"], worktree_root,
        "{compact}"
    );
    assert!(compact["suggested_card_requests"][0]["target"]
        .as_str()
        .unwrap()
        .starts_with("file_"));

    let cards = search_json(&fixture.state, params("wt_unique_token", OutputMode::Cards));
    assert_eq!(cards["cards"][0]["path"], "src/wt_only.rs", "{cards}");
    assert_eq!(cards["cards"][0]["root_id"], worktree_root, "{cards}");
    assert_eq!(cards["cards"][0]["is_primary_root"], false, "{cards}");

    let mut filtered = params("wt_unique_token", OutputMode::Default);
    filtered.path_filter = Some("src/*.rs".to_string());
    filtered.file_type = Some("rs".to_string());
    let filtered = search_json(&fixture.state, filtered);
    assert_eq!(filtered["result_count"], 4, "{filtered}");
}

fn params(query: &str, output_mode: OutputMode) -> SearchParams {
    SearchParams {
        repo_root: None,
        query: query.to_string(),
        limit: 5,
        path_filter: None,
        file_type: None,
        exclude_type: None,
        case_insensitive: false,
        output_mode,
        budget_tokens: None,
        mode: SearchMode::Lexical,
        literal: false,
    }
}

fn search_json(state: &SynrepoState, params: SearchParams) -> Value {
    serde_json::from_str(&handle_search(state, params)).unwrap()
}

struct WorktreeFixture {
    _main: tempfile::TempDir,
    _worktree: tempfile::TempDir,
    state: SynrepoState,
}

fn worktree_fixture() -> WorktreeFixture {
    let main = tempdir().unwrap();
    let worktree = tempdir().unwrap();
    fs::create_dir_all(main.path().join("src")).unwrap();
    fs::write(main.path().join("src/lib.rs"), "pub fn main_root() {}\n").unwrap();

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
    fs::write(
        worktree.path().join("src/wt_only.rs"),
        "pub fn wt_unique_token() {}\n// wt_unique_token one\n// wt_unique_token two\n// wt_unique_token three\n",
    )
    .unwrap();

    bootstrap(main.path(), None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(main.path()).unwrap(),
        repo_root: main.path().to_path_buf(),
    };
    WorktreeFixture {
        _main: main,
        _worktree: worktree,
        state,
    }
}

fn worktree_root_id(state: &SynrepoState) -> String {
    crate::substrate::discover(state.repo_root.as_path(), &state.config)
        .unwrap()
        .into_iter()
        .find(|file| file.relative_path == "src/wt_only.rs")
        .unwrap()
        .root_discriminant
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
