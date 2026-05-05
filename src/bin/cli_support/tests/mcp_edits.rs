use std::fs;

use tempfile::tempdir;

use crate::{cli_support::commands::mcp::SynrepoServer, prepare_mcp_state};
use synrepo::bootstrap::bootstrap;

fn setup_bootstrapped_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    fs::write(repo.join("lib.rs"), "fn main() {}").unwrap();
    bootstrap(repo, None, false).unwrap();
    let repo_path = repo.to_path_buf();
    (dir, repo_path)
}

#[test]
fn mcp_mutating_tools_are_hidden_by_default() {
    let (dir, repo) = setup_bootstrapped_repo();
    let state = prepare_mcp_state(&repo).expect("MCP state should load");
    let server = SynrepoServer::new(state, false);
    let tools = server.registered_tool_names();
    assert!(
        !tools
            .iter()
            .any(|tool| tool == "synrepo_prepare_edit_context"),
        "default MCP must not advertise prepare edit tool"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool == "synrepo_apply_anchor_edits"),
        "default MCP must not advertise apply edit tool"
    );
    assert!(
        tools.iter().any(|tool| tool == "synrepo_context_pack"),
        "read-first tools must remain available"
    );
    assert!(
        !tools.iter().any(|tool| tool == "synrepo_note_add"),
        "default MCP must not advertise advisory note writes"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool == "synrepo_refresh_commentary"),
        "default MCP must not advertise commentary refresh"
    );
    assert!(
        tools.iter().any(|tool| tool == "synrepo_notes"),
        "advisory note reads stay available in default MCP"
    );
    drop(dir);
}

#[test]
fn mcp_source_edit_tools_are_registered_when_allowed() {
    let (dir, repo) = setup_bootstrapped_repo();
    let state = prepare_mcp_state(&repo).expect("MCP state should load");
    let server = SynrepoServer::new(state, true);
    let tools = server.registered_tool_names();
    assert!(
        tools
            .iter()
            .any(|tool| tool == "synrepo_prepare_edit_context"),
        "edit-enabled MCP must advertise prepare edit tool"
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool == "synrepo_apply_anchor_edits"),
        "edit-enabled MCP must advertise apply edit tool"
    );
    drop(dir);
}

#[test]
fn mcp_overlay_write_tools_are_registered_when_allowed() {
    let (dir, repo) = setup_bootstrapped_repo();
    let state = prepare_mcp_state(&repo).expect("MCP state should load");
    let server = SynrepoServer::new_optional_with_overlay(Some(state), true, false);
    let tools = server.registered_tool_names();
    assert!(
        tools.iter().any(|tool| tool == "synrepo_note_add"),
        "overlay-write MCP must advertise advisory note writes"
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool == "synrepo_refresh_commentary"),
        "overlay-write MCP must advertise commentary refresh"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool == "synrepo_apply_anchor_edits"),
        "overlay-write MCP must not imply source-edit tools"
    );
    drop(dir);
}
