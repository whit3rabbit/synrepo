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

    let schema = schemars::schema_for!(synrepo::surface::mcp::context_pack::ContextPackParams);
    let schema_json = serde_json::to_value(schema).expect("schema serializes");
    let targets = schema_json
        .pointer("/schema/properties/targets/items")
        .or_else(|| schema_json.pointer("/properties/targets/items"))
        .or_else(|| schema_json.pointer("/schema/properties/targets/items/$ref"))
        .or_else(|| schema_json.pointer("/properties/targets/items/$ref"))
        .expect("targets has an item schema");
    let serialized = serde_json::to_string(&schema_json).unwrap();
    assert!(
        serialized.contains("\"kind\"") && serialized.contains("\"target\""),
        "context-pack schema must expose target object fields: {schema_json}"
    );
    assert_ne!(
        targets.get("type").and_then(|value| value.as_str()),
        Some("string"),
        "targets must not be exposed as raw strings: {schema_json}"
    );
}

#[test]
fn skill_contains_context_budget_contract() {
    let skill = fs::read_to_string("skill/SKILL.md").expect("read skill");

    assert!(skill.contains("## Context budget contract"));
    assert!(skill.contains("Return the smallest useful MCP response"));
    assert!(skill.contains("Do not request `deep` cards for more than 1-3 files at a time"));
}

#[test]
fn refactor_suggestions_params_and_docs_are_listed() {
    let source = fs::read_to_string("src/surface/mcp/refactor_suggestions.rs")
        .expect("read refactor suggestions source");
    assert!(source.contains("pub min_lines: usize"));
    assert!(source.contains("pub limit: usize"));
    assert!(source.contains("pub path_filter: Option<String>"));

    let docs = fs::read_to_string("docs/MCP.md").expect("read MCP docs");
    assert!(docs.contains("synrepo_refactor_suggestions"));
    assert!(docs.contains("source_store: \"graph+filesystem\""));
}
