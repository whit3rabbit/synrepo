use super::*;

#[test]
fn doctrine_block_size_is_bounded() {
    assert!(!DOCTRINE_BLOCK.is_empty());
    assert!(
        DOCTRINE_BLOCK.len() < 4096,
        "DOCTRINE_BLOCK grew past 4 KiB ({}); consider whether the shim still fits its purpose",
        DOCTRINE_BLOCK.len()
    );
}

#[test]
fn every_shim_embeds_doctrine_block() {
    for tool in [
        AgentTool::Claude,
        AgentTool::Cursor,
        AgentTool::Copilot,
        AgentTool::Generic,
        AgentTool::Codex,
        AgentTool::Windsurf,
        AgentTool::OpenCode,
        AgentTool::Gemini,
        AgentTool::Goose,
        AgentTool::Kiro,
        AgentTool::Qwen,
        AgentTool::Junie,
        AgentTool::Roo,
        AgentTool::Tabnine,
        AgentTool::Trae,
    ] {
        assert!(
            tool.shim_spec_body().contains(DOCTRINE_BLOCK),
            "{} shim does not embed DOCTRINE_BLOCK verbatim",
            tool.display_name()
        );
    }
}

#[test]
fn doctrine_block_covers_required_sections() {
    assert!(DOCTRINE_BLOCK.contains("## Agent doctrine"));
    assert!(DOCTRINE_BLOCK.contains("### Default path"));
    assert!(DOCTRINE_BLOCK.contains("### Do not"));
    assert!(DOCTRINE_BLOCK.contains("### Product boundary"));
    assert!(DOCTRINE_BLOCK.contains("`tiny`"));
    assert!(DOCTRINE_BLOCK.contains("`normal`"));
    assert!(DOCTRINE_BLOCK.contains("`deep`"));
    assert!(DOCTRINE_BLOCK.contains("not a task tracker"));
    assert!(DOCTRINE_BLOCK.contains("Refresh is explicit"));
    assert!(DOCTRINE_BLOCK.contains("### MCP repository selection"));
    assert!(DOCTRINE_BLOCK.contains("synrepo project add <path>"));
}

#[test]
fn doctrine_mentions_global_repo_root_guidance_once() {
    let needle =
        "Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path.";
    assert_eq!(DOCTRINE_BLOCK.matches(needle).count(), 1);
}

#[test]
fn skill_md_includes_doctrine_lines_verbatim() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let skill_path = manifest_dir.join("skill").join("SKILL.md");
    let skill = std::fs::read_to_string(&skill_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", skill_path.display()));

    let required = [
        "Use `tiny` cards to orient and route.",
        "Use `normal` cards to understand a neighborhood.",
        "Use `deep` cards only before writing code, or when exact source or body details matter.",
        "Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.",
        "Do not treat overlay commentary as canonical. It is advisory prose layered on structural cards.",
        "Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.",
        "Do not expect watch or background behavior unless `synrepo watch` is explicitly running.",
        "synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.",
        "Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.",
        "Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.",
        "Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path.",
        "If a tool reports that a repository is not managed by synrepo, ask the user to run `synrepo project add <path>`; do not bypass registry gating.",
    ];

    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|line| !skill.contains(line))
        .collect();
    assert!(
        missing.is_empty(),
        "skill/SKILL.md is missing {} doctrine line(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn card_returning_mcp_tool_descriptions_share_escalation_line() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let main_path = manifest_dir
        .join("src")
        .join("bin")
        .join("cli_support")
        .join("commands")
        .join("mcp")
        .join("tools.rs");
    let source = std::fs::read_to_string(&main_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", main_path.display()));

    let card_returning = [
        "synrepo_card",
        "synrepo_where_to_edit",
        "synrepo_change_impact",
        "synrepo_entrypoints",
        "synrepo_module_card",
        "synrepo_public_api",
        "synrepo_minimum_context",
    ];

    for tool in card_returning {
        let needle = format!("name = \"{tool}\"");
        let idx = source
            .find(&needle)
            .unwrap_or_else(|| panic!("did not find `{needle}` in MCP main.rs"));
        let window_end = (idx + 800).min(source.len());
        let window = &source[idx..window_end];
        assert!(
            window.contains(TOOL_DESC_ESCALATION_LINE),
            "card-returning MCP tool `{tool}` description does not contain TOOL_DESC_ESCALATION_LINE"
        );
    }

    let non_card = [
        "synrepo_search",
        "synrepo_overview",
        "synrepo_findings",
        "synrepo_recent_activity",
    ];

    for tool in non_card {
        let needle = format!("name = \"{tool}\"");
        let idx = source
            .find(&needle)
            .unwrap_or_else(|| panic!("did not find `{needle}` in MCP main.rs"));
        let window_end = (idx + 800).min(source.len());
        let window = &source[idx..window_end];
        assert!(
            !window.contains(TOOL_DESC_ESCALATION_LINE),
            "non-card MCP tool `{tool}` description carries the escalation sentence; remove it (the default-budget semantics for this tool differ from card-returning tools)"
        );
    }
}
