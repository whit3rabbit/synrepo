use std::fs;

#[test]
fn mcp_source_registers_docs_search_tool() {
    let source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");
    assert!(
        source.contains("name = \"synrepo_docs_search\""),
        "MCP registration must include synrepo_docs_search"
    );
}

#[test]
fn mcp_source_registers_context_pack_and_resources() {
    let tools_source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");
    assert!(
        tools_source.contains("name = \"synrepo_ask\""),
        "MCP registration must include synrepo_ask"
    );
    assert!(
        tools_source.contains("name = \"synrepo_context_pack\""),
        "MCP registration must include synrepo_context_pack"
    );
    let source =
        fs::read_to_string("src/bin/cli_support/commands/mcp.rs").expect("read MCP server source");
    assert!(
        source.contains(".enable_resources()"),
        "MCP server must advertise resource support"
    );
    assert!(
        source.contains("synrepo://file/{path}/outline"),
        "MCP resource templates must include file outlines"
    );
    assert!(
        source.contains("synrepo://project/{project_id}/card/{target}"),
        "MCP resource templates must include project-qualified cards"
    );
    assert!(
        source.contains("synrepo://project/{project_id}/file/{path}/outline"),
        "MCP resource templates must include project-qualified file outlines"
    );
    assert!(
        source.contains("synrepo://project/{project_id}/context-pack?goal={goal}"),
        "MCP resource templates must include project-qualified context packs"
    );
    assert!(
        source.contains("synrepo://projects"),
        "MCP resource templates must include managed projects"
    );
}

#[test]
fn mcp_source_registers_metrics_and_project_tools() {
    let source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");
    for tool in [
        "synrepo_readiness",
        "synrepo_metrics",
        "synrepo_use_project",
    ] {
        let needle = format!("name = \"{tool}\"");
        assert!(
            source.contains(&needle),
            "MCP registration must include {tool}"
        );
    }
}

#[test]
fn mcp_source_registers_refactor_suggestions_tool() {
    let source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");
    assert!(
        source.contains("name = \"synrepo_refactor_suggestions\""),
        "MCP registration must include synrepo_refactor_suggestions"
    );
}

#[test]
fn mcp_source_registers_workflow_aliases() {
    let source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");
    for alias in [
        "synrepo_orient",
        "synrepo_find",
        "synrepo_explain",
        "synrepo_impact",
        "synrepo_risks",
        "synrepo_tests",
        "synrepo_changed",
        "synrepo_resume_context",
    ] {
        let needle = format!("name = \"{alias}\"");
        assert!(
            source.contains(&needle),
            "MCP registration must include {alias}"
        );
    }
}
