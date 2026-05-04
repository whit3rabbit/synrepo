use std::fs;

use tempfile::tempdir;

use super::{collect_refactor_suggestions_for_repo, RefactorSuggestionOptions, DEFAULT_MIN_LINES};

fn make_repo(files: &[(&str, String)]) -> (tempfile::TempDir, std::path::PathBuf) {
    let home = tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let dir = tempdir().unwrap();
    for (path, body) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    crate::bootstrap::bootstrap(dir.path(), None, false).unwrap();
    let repo = dir.path().to_path_buf();
    (dir, repo)
}

fn rust_lines(lines: usize, name: &str) -> String {
    let mut body = format!("pub fn {name}() {{}}\n");
    for idx in 1..lines {
        body.push_str(&format!("// {name} {idx}\n"));
    }
    body
}

#[test]
fn threshold_excludes_exact_count_and_includes_above_count() {
    let (_dir, repo) = make_repo(&[
        ("src/exact.rs", rust_lines(DEFAULT_MIN_LINES, "exact")),
        ("src/above.rs", rust_lines(DEFAULT_MIN_LINES + 1, "above")),
    ]);

    let report =
        collect_refactor_suggestions_for_repo(&repo, RefactorSuggestionOptions::default()).unwrap();

    let paths: Vec<_> = report
        .candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect();
    assert_eq!(paths, vec!["src/above.rs"]);
    assert_eq!(report.threshold, DEFAULT_MIN_LINES);
    assert_eq!(report.metric, "physical_lines");
    assert_eq!(report.source_store, "graph+filesystem");
}

#[test]
fn excludes_test_paths_and_sorts_by_line_count_then_path() {
    let (_dir, repo) = make_repo(&[
        ("src/b.rs", rust_lines(330, "b")),
        ("src/a.rs", rust_lines(330, "a")),
        ("src/tests/large.rs", rust_lines(500, "large_test_dir")),
        ("src/large_test.rs", rust_lines(500, "large_test_file")),
        ("src/large_tests.rs", rust_lines(500, "large_tests_file")),
    ]);

    let report =
        collect_refactor_suggestions_for_repo(&repo, RefactorSuggestionOptions::default()).unwrap();

    let paths: Vec<_> = report
        .candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect();
    assert_eq!(paths, vec!["src/a.rs", "src/b.rs"]);
    assert_eq!(report.candidate_count, 2);
    assert_eq!(report.groups[0].language, "rust");
    assert_eq!(report.groups[0].count, 2);
}

#[test]
fn respects_limit_and_path_filter() {
    let (_dir, repo) = make_repo(&[
        ("src/keep/a.rs", rust_lines(340, "keep_a")),
        ("src/keep/b.rs", rust_lines(330, "keep_b")),
        ("src/drop/c.rs", rust_lines(500, "drop_c")),
    ]);
    let options = RefactorSuggestionOptions {
        limit: 1,
        path_filter: Some("src/keep/".to_string()),
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();

    assert_eq!(report.candidate_count, 2);
    assert_eq!(report.omitted_count, 1);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].path, "src/keep/a.rs");
}

#[test]
fn glob_path_filter_matches_candidates() {
    let (_dir, repo) = make_repo(&[
        ("src/app/main.rs", rust_lines(340, "app_main")),
        ("src/lib/main.rs", rust_lines(330, "lib_main")),
    ]);
    let options = RefactorSuggestionOptions {
        path_filter: Some("src/app/*.rs".to_string()),
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();

    assert_eq!(report.candidate_count, 1);
    assert_eq!(report.candidates[0].path, "src/app/main.rs");
}
