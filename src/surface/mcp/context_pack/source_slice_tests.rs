use std::fs;

use tempfile::tempdir;

use crate::bootstrap::bootstrap;
use crate::config::Config;
use crate::surface::mcp::compact::OutputMode;
use crate::surface::mcp::SynrepoState;

use super::{build_context_pack, ContextPackParams, ContextPackTarget};

fn make_state() -> (tempfile::TempDir, SynrepoState) {
    let home = tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let dir = tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn alpha() {\n    println!(\"alpha\");\n}\n\npub fn beta() {\n    alpha();\n}\n",
    )
    .unwrap();
    bootstrap(repo, None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(repo).unwrap(),
        repo_root: repo.to_path_buf(),
    };
    (dir, state)
}

#[test]
fn source_slice_returns_line_numbered_cluster() {
    let (_dir, state) = make_state();
    let value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![ContextPackTarget {
                kind: "source_slice".to_string(),
                target: "beta".to_string(),
                budget: Some("normal".to_string()),
            }],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap();

    let artifact = &value["artifacts"][0];
    assert_eq!(artifact["artifact_type"], "source_slice");
    assert_eq!(artifact["content"]["slice_state"], "fresh");
    let rendered = artifact["content"]["files"][0]["rendered_source"]
        .as_str()
        .unwrap();
    assert!(rendered.contains("5\tpub fn beta()"));
    assert!(rendered.contains("6\t    alpha();"));
}

#[test]
fn source_slice_omits_when_file_hash_is_stale() {
    let (dir, state) = make_state();
    fs::write(dir.path().join("src/lib.rs"), "pub fn beta() {}\n").unwrap();

    let value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![ContextPackTarget {
                kind: "source_slice".to_string(),
                target: "src/lib.rs".to_string(),
                budget: Some("normal".to_string()),
            }],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap();

    let content = &value["artifacts"][0]["content"];
    assert_eq!(content["slice_state"], "stale_omitted");
    assert_eq!(content["omitted"][0]["reason"], "content_hash_mismatch");
    assert_eq!(value["context_state"]["stale"], true);
}
