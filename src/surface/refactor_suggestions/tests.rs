use std::fs;

use tempfile::tempdir;

use super::{
    collect_refactor_suggestions_for_repo, RefactorSuggestionMode, RefactorSuggestionOptions,
    RefactorSuggestionReport, DEFAULT_MIN_LINES, DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT,
};

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

fn collect_missing_docs(files: &[(&str, String)]) -> (tempfile::TempDir, RefactorSuggestionReport) {
    let (dir, repo) = make_repo(files);
    let options = RefactorSuggestionOptions {
        mode: RefactorSuggestionMode::MissingDocs,
        ..RefactorSuggestionOptions::default()
    };
    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();
    (dir, report)
}

fn only_missing_name(report: &RefactorSuggestionReport) -> &str {
    assert_eq!(report.candidate_count, 1);
    assert_eq!(report.candidates[0].missing_public_doc_count, 1);
    report.candidates[0].missing_public_docs[0]
        .qualified_name
        .as_str()
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
    assert_eq!(report.criteria.line_count_threshold, DEFAULT_MIN_LINES);
    assert!(report.criteria.line_count_threshold_applied);
    assert_eq!(report.mode, RefactorSuggestionMode::LineCount);
    assert_eq!(report.metric, "physical_lines");
    assert_eq!(report.source_store, "graph+filesystem");
    assert_eq!(report.candidates[0].missing_public_doc_count, 0);
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

#[test]
fn missing_docs_reports_public_undocumented_rust_symbol() {
    let (_dir, repo) = make_repo(&[(
        "src/lib.rs",
        "pub fn missing_docs() {}\nfn private_helper() {}\n".to_string(),
    )]);
    let options = RefactorSuggestionOptions {
        mode: RefactorSuggestionMode::MissingDocs,
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();

    assert_eq!(report.mode, RefactorSuggestionMode::MissingDocs);
    assert_eq!(report.metric, "missing_public_docs");
    assert_eq!(report.threshold, DEFAULT_MIN_LINES);
    assert!(!report.criteria.line_count_threshold_applied);
    assert_eq!(report.criteria.visibility, Some("public"));
    assert_eq!(report.criteria.doc_source, Some("ast_doc_comment"));
    assert_eq!(report.candidate_count, 1);
    assert_eq!(report.candidates[0].path, "src/lib.rs");
    assert_eq!(report.candidates[0].missing_public_doc_count, 1);
    assert_eq!(
        report.candidates[0].missing_public_docs[0].qualified_name,
        "missing_docs"
    );
    assert!(report.candidates[0].missing_public_docs[0]
        .symbol_id
        .to_string()
        .starts_with("sym_"));
}

#[test]
fn missing_docs_ignores_private_undocumented_symbol() {
    let (_dir, repo) = make_repo(&[("src/lib.rs", "fn private_helper() {}\n".to_string())]);
    let options = RefactorSuggestionOptions {
        mode: RefactorSuggestionMode::MissingDocs,
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();

    assert_eq!(report.candidate_count, 0);
    assert!(report.candidates.is_empty());
}

#[test]
fn missing_docs_ignores_documented_public_symbol() {
    let (_dir, repo) = make_repo(&[(
        "src/lib.rs",
        "/// Existing docs.\npub fn documented() {}\n".to_string(),
    )]);
    let options = RefactorSuggestionOptions {
        mode: RefactorSuggestionMode::MissingDocs,
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();

    assert_eq!(report.candidate_count, 0);
    assert!(report.candidates.is_empty());
}

#[test]
fn missing_docs_reports_undocumented_java_symbol() {
    let (_dir, repo) = make_repo(&[("src/Greeter.java", "public class Greeter {}\n".to_string())]);
    let options = RefactorSuggestionOptions {
        mode: RefactorSuggestionMode::MissingDocs,
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();

    assert_eq!(report.candidate_count, 1);
    assert_eq!(report.candidates[0].language.as_deref(), Some("java"));
    assert_eq!(report.candidates[0].missing_public_doc_count, 1);
    assert_eq!(
        report.candidates[0].missing_public_docs[0].qualified_name,
        "Greeter"
    );
}

#[test]
fn missing_docs_ignores_documented_java_symbol() {
    let (_dir, repo) = make_repo(&[(
        "src/Greeter.java",
        "/** Existing docs. */\npublic class Greeter {}\n".to_string(),
    )]);
    let options = RefactorSuggestionOptions {
        mode: RefactorSuggestionMode::MissingDocs,
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();

    assert_eq!(report.candidate_count, 0);
    assert!(report.candidates.is_empty());
}

#[test]
fn missing_docs_ignores_unexported_javascript_helper() {
    let (_dir, report) =
        collect_missing_docs(&[("src/helper.js", "function helper() {}\n".to_string())]);

    assert_eq!(report.candidate_count, 0);
    assert!(report.candidates.is_empty());
}

#[test]
fn missing_docs_reports_esm_javascript_export() {
    let (_dir, report) = collect_missing_docs(&[(
        "src/api.js",
        "export function exposed() {}\nfunction helper() {}\n".to_string(),
    )]);

    assert_eq!(only_missing_name(&report), "exposed");
}

#[test]
fn missing_docs_reports_commonjs_export() {
    let (_dir, report) = collect_missing_docs(&[(
        "src/api.cjs",
        "function exposed() {}\nmodule.exports.exposed = exposed;\n".to_string(),
    )]);

    assert_eq!(only_missing_name(&report), "exposed");
}

#[test]
fn missing_docs_ignores_exported_javascript_test_paths() {
    let (_dir, report) = collect_missing_docs(&[(
        "src/api.test.js",
        "export function exposed() {}\n".to_string(),
    )]);

    assert_eq!(report.candidate_count, 0);
    assert!(report.candidates.is_empty());
}

#[test]
fn missing_docs_ignores_python_script_helper() {
    let (_dir, report) = collect_missing_docs(&[(
        "tools/script.py",
        "def helper():\n    return None\n".to_string(),
    )]);

    assert_eq!(report.candidate_count, 0);
    assert!(report.candidates.is_empty());
}

#[test]
fn missing_docs_reports_python_all_symbol() {
    let (_dir, report) = collect_missing_docs(&[(
        "pkg/helpers.py",
        "__all__ = ['public_helper']\n\
         def public_helper():\n    return None\n\
         def internal_helper():\n    return None\n"
            .to_string(),
    )]);

    assert_eq!(only_missing_name(&report), "public_helper");
}

#[test]
fn missing_docs_reports_python_init_direct_api() {
    let (_dir, report) = collect_missing_docs(&[(
        "pkg/__init__.py",
        "def exported():\n    return None\n".to_string(),
    )]);

    assert_eq!(only_missing_name(&report), "exported");
}

#[test]
fn missing_docs_reports_python_package_reexport() {
    let (_dir, report) = collect_missing_docs(&[
        (
            "pkg/__init__.py",
            "from .helpers import PublicHelper\n".to_string(),
        ),
        (
            "pkg/helpers.py",
            "class PublicHelper:\n    pass\n".to_string(),
        ),
    ]);

    assert_eq!(report.candidates[0].path, "pkg/helpers.py");
    assert_eq!(only_missing_name(&report), "PublicHelper");
}

#[test]
fn missing_docs_preview_is_bounded() {
    let mut body = String::new();
    for idx in 0..(DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT + 2) {
        body.push_str(&format!("pub fn missing_{idx}() {{}}\n"));
    }
    let (_dir, repo) = make_repo(&[("src/lib.rs", body)]);
    let options = RefactorSuggestionOptions {
        mode: RefactorSuggestionMode::MissingDocs,
        ..RefactorSuggestionOptions::default()
    };

    let report = collect_refactor_suggestions_for_repo(&repo, options).unwrap();
    let candidate = &report.candidates[0];

    assert_eq!(
        candidate.missing_public_doc_count,
        DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT + 2
    );
    assert_eq!(
        candidate.missing_public_docs.len(),
        DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT
    );
    assert_eq!(candidate.missing_public_docs_omitted, 2);
}
