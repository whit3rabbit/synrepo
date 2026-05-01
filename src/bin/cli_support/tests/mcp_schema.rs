use std::fs;

#[test]
fn context_pack_tool_description_names_structured_targets() {
    let tools_source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");

    assert!(
        tools_source.contains("Pass targets as structured objects: {kind,target,budget?}"),
        "context-pack MCP description must tell agents to pass structured targets"
    );
    assert!(
        tools_source
            .contains("file, symbol, directory, minimum_context, test_surface, call_path, search"),
        "context-pack MCP description must list supported target kinds"
    );
}

#[test]
fn context_pack_params_schema_uses_target_objects() {
    let source =
        fs::read_to_string("src/surface/mcp/context_pack.rs").expect("read context pack source");

    assert!(
        source.contains("pub targets: Vec<ContextPackTarget>"),
        "context-pack params must expose structured target objects, not raw strings"
    );
    assert!(source.contains("pub kind: String"));
    assert!(source.contains("pub target: String"));
    assert!(source.contains("pub budget: Option<String>"));
}
