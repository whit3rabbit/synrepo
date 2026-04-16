use super::super::commands::{search, search_output};
use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use syntext::SearchOptions;
use tempfile::tempdir;

/// Construct a small test repo with a `.rs`, a `.md`, and a `.py` file all
/// containing the token "TokenAlpha" in distinct paths.
fn three_file_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn main() { /* TokenAlpha */ }\n").unwrap();
    std::fs::write(
        repo.join("docs/b.md"),
        "# heading\n\nTokenAlpha appears here\n",
    )
    .unwrap();
    std::fs::write(repo.join("src/c.py"), "x = 'TokenAlpha'\n").unwrap();
    bootstrap(&repo, None).unwrap();
    (dir, repo)
}

#[test]
fn search_requires_rebuild_when_index_sensitive_config_changes() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "search token\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let updated = Config {
        roots: vec!["src".to_string()],
        ..Config::load(repo.path()).unwrap()
    };
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        toml::to_string_pretty(&updated).unwrap(),
    )
    .unwrap();

    let error = search(repo.path(), "search token", SearchOptions::default())
        .unwrap_err()
        .to_string();

    assert!(error.contains("Storage compatibility"));
    assert!(error.contains("requires rebuild"));
}

#[test]
fn search_refuses_to_race_the_writer_lock() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "search token\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .unwrap();
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string(&WriterOwnership {
            pid: child.id(),
            acquired_at: "now".to_string(),
        })
        .unwrap(),
    )
    .unwrap();

    let error = search(repo.path(), "search token", SearchOptions::default())
        .unwrap_err()
        .to_string();

    assert!(error.contains("writer lock"));
    assert!(error.contains("retry"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn search_case_insensitive_flag_changes_results() {
    let (_dir, repo) = three_file_repo();

    // Case-sensitive search for the lowercase form must miss the `TokenAlpha` literal.
    let sensitive = search_output(
        &repo,
        "tokenalpha",
        SearchOptions {
            case_insensitive: false,
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(
        sensitive.contains("No matches found for `tokenalpha`."),
        "expected no matches message with case_insensitive=false, got: {sensitive}"
    );

    // Case-insensitive must hit all three files.
    let insensitive = search_output(
        &repo,
        "tokenalpha",
        SearchOptions {
            case_insensitive: true,
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(
        insensitive.contains("Found 3 matches."),
        "expected 3 matches with case_insensitive=true, got: {insensitive}"
    );
}

#[test]
fn search_file_type_include_filters_to_matching_extensions() {
    let (_dir, repo) = three_file_repo();

    let out = search_output(
        &repo,
        "TokenAlpha",
        SearchOptions {
            file_type: Some("rs".into()),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(
        out.contains("Found 1 matches."),
        "expected only the .rs file, got: {out}"
    );
    assert!(
        out.contains("src/a.rs"),
        "expected src/a.rs in output, got: {out}"
    );
    assert!(!out.contains("b.md") && !out.contains("c.py"));
}

#[test]
fn search_exclude_type_filters_out_matching_extensions() {
    let (_dir, repo) = three_file_repo();

    let out = search_output(
        &repo,
        "TokenAlpha",
        SearchOptions {
            exclude_type: Some("md".into()),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(
        out.contains("Found 2 matches."),
        "expected 2 matches (md excluded), got: {out}"
    );
    assert!(!out.contains("b.md"), "md must be excluded, got: {out}");
}

#[test]
fn search_path_filter_limits_to_matching_paths() {
    let (_dir, repo) = three_file_repo();

    let out = search_output(
        &repo,
        "TokenAlpha",
        SearchOptions {
            // syntext globs: `docs/` is a directory-prefix filter; `**/...`
            // is not supported.
            path_filter: Some("docs/".into()),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(
        out.contains("Found 1 matches."),
        "expected only docs/b.md, got: {out}"
    );
    assert!(
        out.contains("docs/b.md"),
        "expected docs/b.md in output, got: {out}"
    );
}

#[test]
fn search_max_results_truncates_output() {
    let (_dir, repo) = three_file_repo();

    let out = search_output(
        &repo,
        "TokenAlpha",
        SearchOptions {
            max_results: Some(2),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(
        out.contains("Found 2 matches."),
        "expected max_results=2 to truncate, got: {out}"
    );
}

#[test]
fn search_glob_path_filter_matches_correct_files() {
    let (_dir, repo) = three_file_repo();

    // 1. Glob for .rs files
    let out_rs = search_output(
        &repo,
        "TokenAlpha",
        SearchOptions {
            path_filter: Some("**/*.rs".into()),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(out_rs.contains("Found 1 matches."));
    assert!(out_rs.contains("src/a.rs"));

    // 2. Glob for src/ directory
    let out_src = search_output(
        &repo,
        "TokenAlpha",
        SearchOptions {
            path_filter: Some("src/**/*.py".into()),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(out_src.contains("Found 1 matches."));
    assert!(out_src.contains("src/c.py"));
}
