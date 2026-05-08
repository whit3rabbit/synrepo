use std::fs;

use serde_json::Value;
use tempfile::tempdir;

use super::super::commands::resume_context_output;
use super::support::{bootstrap_isolated as bootstrap, git};

#[test]
fn resume_context_json_outputs_changed_files() {
    let repo = tempdir().unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.path().join("lib.rs"), "fn main() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "init"]);
    bootstrap(repo.path(), None, false).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "bootstrap"]);
    fs::write(
        repo.path().join("lib.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();

    let output = resume_context_output(repo.path(), Some(5), Some(7), Some(1_500), true, true)
        .expect("resume-context JSON should render");
    let json: Value = serde_json::from_str(&output).expect("output should be JSON");

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["packet_type"], "repo_resume_context");
    assert_eq!(json["sections"]["changed_files"]["files"][0], "lib.rs");
    assert_eq!(json["sections"]["saved_notes"]["overlay_state"], "disabled");
    assert_eq!(json["context_state"]["token_cap"], 1_500);
}

#[test]
fn resume_context_markdown_renders_detail_pointers() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), None, false).unwrap();

    let output = resume_context_output(repo.path(), None, None, None, false, false)
        .expect("resume-context markdown should render");

    assert!(output.contains("# Repo Resume Context"));
    assert!(output.contains("## Detail Pointers"));
    assert!(output.contains("synrepo status --recent"));
}
