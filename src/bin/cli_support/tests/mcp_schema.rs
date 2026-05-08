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
fn ask_params_schema_exposes_task_context_controls() {
    let schema = schemars::schema_for!(synrepo::surface::mcp::ask::AskParams);
    let schema_json = serde_json::to_value(schema).expect("schema serializes");
    let serialized = serde_json::to_string(&schema_json).unwrap();

    for field in [
        "\"ask\"",
        "\"scope\"",
        "\"paths\"",
        "\"symbols\"",
        "\"shape\"",
        "\"ground\"",
        "\"budget\"",
        "\"max_tokens\"",
    ] {
        assert!(
            serialized.contains(field),
            "ask schema must expose {field}: {schema_json}"
        );
    }
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
fn context_pack_tool_description_lists_task_context_artifacts() {
    let tools_source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");

    for kind in [
        "entrypoints",
        "public_api",
        "change_risk",
        "findings",
        "recent_activity",
    ] {
        assert!(
            tools_source.contains(kind),
            "context-pack MCP description must list {kind}"
        );
    }
}

#[test]
fn skill_contains_context_budget_contract() {
    let skill = fs::read_to_string("skill/SKILL.md").expect("read skill");
    let budget_reference = fs::read_to_string("skill/references/budgets-and-errors.md")
        .expect("read budget reference");

    assert!(skill.contains("## Context budget contract"));
    assert!(skill.contains("Return the smallest useful MCP response"));
    assert!(skill.contains("Do not request `deep` cards for more than 1-3 files at a time"));
    assert!(skill.contains("references/budgets-and-errors.md"));

    assert!(budget_reference.contains("## Budget protocol"));
    assert!(budget_reference.contains("Default sequence:"));
    assert!(budget_reference.contains("`synrepo_resume_context`: `context_state`, `sections.changed_files`, `sections.next_actions`, `detail_pointers`, `omitted`"));
    assert!(budget_reference.contains("MCP errors are structured."));
    assert!(budget_reference.contains("If you receive `RATE_LIMITED`, wait briefly or reduce batching."));
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

#[test]
fn resume_context_params_and_docs_are_listed() {
    let tools_source = fs::read_to_string("src/bin/cli_support/commands/mcp/tools.rs")
        .expect("read MCP registration source");
    assert!(tools_source.contains("name = \"synrepo_resume_context\""));
    assert!(tools_source.contains("NOT session memory"));

    let schema = schemars::schema_for!(synrepo::surface::mcp::resume_context::ResumeContextParams);
    let schema_json = serde_json::to_value(schema).expect("schema serializes");
    let serialized = serde_json::to_string(&schema_json).unwrap();
    for field in [
        "\"repo_root\"",
        "\"limit\"",
        "\"since_days\"",
        "\"budget_tokens\"",
        "\"include_notes\"",
    ] {
        assert!(
            serialized.contains(field),
            "resume-context schema must expose {field}: {schema_json}"
        );
    }

    let docs = fs::read_to_string("docs/MCP.md").expect("read MCP docs");
    assert!(docs.contains("synrepo_resume_context"));
    assert!(docs.contains("prompt logs, chat history, raw tool outputs"));
}
