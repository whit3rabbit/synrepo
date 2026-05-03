use serde_json::{json, Value};
use synrepo::config::Config;
use synrepo::surface::mcp::compact::OutputMode;
use synrepo::surface::mcp::search::{handle_search, SearchParams};
use synrepo::surface::mcp::SynrepoState;
use tempfile::{tempdir, TempDir};

use super::support::bootstrap_isolated as bootstrap;

fn search_fixture() -> (TempDir, SynrepoState) {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(
        repo.join("src/a.rs"),
        "fn main() { /* TokenAlpha rust */ }\n",
    )
    .unwrap();
    std::fs::write(repo.join("docs/b.md"), "# heading\n\nTokenAlpha docs\n").unwrap();
    std::fs::write(repo.join("src/c.py"), "value = 'TokenAlpha python'\n").unwrap();

    bootstrap(repo, None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(repo).unwrap(),
        repo_root: repo.to_path_buf(),
    };
    (dir, state)
}

fn params(query: &str) -> SearchParams {
    SearchParams {
        repo_root: None,
        query: query.to_string(),
        limit: 20,
        path_filter: None,
        file_type: None,
        exclude_type: None,
        case_insensitive: false,
        output_mode: OutputMode::Default,
        budget_tokens: None,
    }
}

fn search_json(state: &SynrepoState, params: SearchParams) -> Value {
    serde_json::from_str(&handle_search(state, params)).unwrap()
}

fn result_paths(output: &Value) -> Vec<String> {
    output["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn mcp_search_preserves_minimal_contract_and_adds_metadata() {
    let (_dir, state) = search_fixture();
    let mut request = params("TokenAlpha");
    request.limit = 2;

    let output = search_json(&state, request);

    assert_eq!(output["query"], "TokenAlpha");
    assert_eq!(output["engine"], "syntext");
    assert_eq!(output["source_store"], "substrate_index");
    assert_eq!(output["limit"], 2);
    assert_eq!(output["result_count"], 2);
    assert_eq!(output["results"].as_array().unwrap().len(), 2);
    assert_eq!(output["filters"]["path_filter"], Value::Null);
    assert_eq!(output["filters"]["case_insensitive"], false);
}

#[test]
fn mcp_search_applies_path_and_type_filters() {
    let (_dir, state) = search_fixture();

    let mut path_request = params("TokenAlpha");
    path_request.path_filter = Some("docs/".to_string());
    let path_output = search_json(&state, path_request);
    assert_eq!(result_paths(&path_output), vec!["docs/b.md"]);

    let mut type_request = params("TokenAlpha");
    type_request.file_type = Some("rs".to_string());
    let type_output = search_json(&state, type_request);
    assert_eq!(result_paths(&type_output), vec!["src/a.rs"]);

    let mut exclude_request = params("TokenAlpha");
    exclude_request.exclude_type = Some("md".to_string());
    let exclude_output = search_json(&state, exclude_request);
    let paths = result_paths(&exclude_output);
    assert_eq!(paths.len(), 2);
    assert!(!paths.contains(&"docs/b.md".to_string()));
}

#[test]
fn mcp_search_applies_glob_filter() {
    let (_dir, state) = search_fixture();
    let mut request = params("TokenAlpha");
    request.path_filter = Some("src/**/*.py".to_string());

    let output = search_json(&state, request);

    assert_eq!(result_paths(&output), vec!["src/c.py"]);
}

#[test]
fn mcp_search_applies_case_insensitive_and_ignore_case_alias() {
    let (_dir, state) = search_fixture();

    let sensitive = search_json(&state, params("tokenalpha"));
    assert!(sensitive["results"].as_array().unwrap().is_empty());

    let mut insensitive = params("tokenalpha");
    insensitive.case_insensitive = true;
    let insensitive_output = search_json(&state, insensitive);
    assert_eq!(insensitive_output["result_count"], 3);

    let alias_params: SearchParams = serde_json::from_value(json!({
        "query": "tokenalpha",
        "ignore_case": true
    }))
    .unwrap();
    let alias_output = search_json(&state, alias_params);
    assert_eq!(alias_output["result_count"], 3);
    assert_eq!(alias_output["filters"]["case_insensitive"], true);
}
