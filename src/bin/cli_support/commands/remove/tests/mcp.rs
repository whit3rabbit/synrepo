use super::*;

#[test]
fn apply_strip_codex_mcp_entry_preserves_other_servers() {
    let fx = Fixture::new();
    fs::create_dir_all(fx.path().join(".codex")).unwrap();
    fs::write(
        fx.path().join(".codex").join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n\n[mcp_servers.other]\ncommand = \"other\"\n",
    )
    .unwrap();

    let plan = build_plan(fx.path(), Some(AgentTool::Codex), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    let raw = fs::read_to_string(fx.path().join(".codex").join("config.toml")).unwrap();
    let v: toml::Value = toml::from_str(&raw).unwrap();
    assert!(
        v.get("mcp_servers")
            .and_then(|servers| servers.get("synrepo"))
            .is_none(),
        "synrepo server entry must be removed"
    );
    assert_eq!(
        v["mcp_servers"]["other"]["command"].as_str().unwrap(),
        "other",
        "other server entry must survive"
    );
}

#[test]
fn apply_strip_mcp_entry_preserves_other_servers() {
    let fx = Fixture::new();
    write_mcp_json_with_synrepo(
        fx.path(),
        Some(("other", json!({ "command": "other-bin", "args": [] }))),
    );

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    let raw = fs::read_to_string(fx.path().join(".mcp.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        v["mcpServers"]["other"].is_object(),
        "other server entry must survive removal"
    );
    assert!(
        v["mcpServers"].get("synrepo").is_none(),
        "synrepo entry must be removed"
    );
}

#[test]
fn apply_strip_mcp_entry_drops_empty_container_but_keeps_file() {
    let fx = Fixture::new();
    write_mcp_json_with_synrepo(fx.path(), None);

    let plan = build_plan(fx.path(), Some(AgentTool::Claude), false).unwrap();
    apply_plan(fx.path(), &plan).unwrap();

    let raw = fs::read_to_string(fx.path().join(".mcp.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        v.get("mcpServers").is_none(),
        "empty mcpServers should be removed"
    );
    assert!(
        fx.path().join(".mcp.json").exists(),
        "file itself must remain"
    );
}
