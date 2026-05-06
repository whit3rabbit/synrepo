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
    assert!(DOCTRINE_BLOCK.contains("local, deterministic code-context compiler"));
    assert!(DOCTRINE_BLOCK
        .contains("repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP"));
    assert!(DOCTRINE_BLOCK.contains("### Default path"));
    assert!(DOCTRINE_BLOCK.contains("`synrepo_ask`"));
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
        "Use `tiny` cards to route and `normal` cards to understand.",
        "Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient.",
        "Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.",
        "Do not treat overlay commentary, explain docs, or proposed cross-links as canonical source truth.",
        "Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.",
        "Do not expect watch or background behavior unless `synrepo watch` is explicitly running.",
        "synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.",
        "Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.",
        "Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.",
        "Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows.",
        "Do not mistake client-side hook nudges for MCP interception or enforcement. They are non-blocking reminders.",
        "Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path.",
        "If a tool reports that a repository is not managed by synrepo, ask the user to run:",
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
fn skill_teaches_exact_identifier_search_before_task_routing() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let skill_path = manifest_dir.join("skill").join("SKILL.md");
    let skill = std::fs::read_to_string(&skill_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", skill_path.display()));

    let required = [
        "For exact symbols, tool names, function names, flags, JSON keys, CLI args, error strings, or file paths, prefer:",
        "`synrepo_search(query, limit?, output_mode?, budget_tokens?)`",
        "Use `output_mode: \"compact\"` for orientation.",
        "Do not use a full sentence when an exact token or string literal is known.",
        "For plain-language edit or investigation tasks, call:",
        "`synrepo_find(task, limit?, budget_tokens?)`",
        "`synrepo_where_to_edit(task, limit?)`",
        "`query_attempts`",
        "`fallback_used`",
        "`miss_reason`",
        "If `miss_reason` is `no_index_matches`, do not retry the same broad sentence.",
    ];

    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|line| !skill.contains(line))
        .collect();
    assert!(
        missing.is_empty(),
        "skill/SKILL.md is missing {} exact-search guidance line(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn skill_surfaces_teach_context_compiler_front_door() {
    let repo_skill = read_repo_file(&["skill", "SKILL.md"]);
    let codex_shim = AgentTool::Codex.shim_spec_body().to_string();
    let tracked_codex_skill = read_repo_file(&[".agents", "skills", "synrepo", "SKILL.md"]);
    let required = [
        "local, deterministic code-context compiler",
        "repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP",
        "`synrepo_ask(ask, scope?, shape?, ground?, budget?)`",
        "default high-level front door for one bounded, cited task-context packet",
        "`answer`, `cards_used`, `evidence`, `grounding`, `omitted_context_notes`, `next_best_tools`, and `context_packet`",
        "`mode` or `citations`, `include_spans`, and `allow_overlay`",
        "Graph facts are authoritative observed source truth",
        "Overlay commentary, explain docs, and notes are advisory",
        "LLM-authored output never mutates the canonical graph",
        "Embeddings are optional routing/search helpers",
    ];

    for (name, surface) in [
        ("skill/SKILL.md", repo_skill.as_str()),
        ("AgentTool::Codex shim", codex_shim.as_str()),
        (
            ".agents/skills/synrepo/SKILL.md",
            tracked_codex_skill.as_str(),
        ),
    ] {
        assert_required_lines(name, surface, &required);
    }
}

#[test]
fn tracked_codex_skill_matches_generated_shim() {
    let tracked_codex_skill = read_repo_file(&[".agents", "skills", "synrepo", "SKILL.md"]);

    assert_eq!(tracked_codex_skill, AgentTool::Codex.shim_spec_body());
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

#[test]
fn mcp_tool_descriptions_distinguish_exact_search_from_task_routing() {
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

    let search_window = tool_window(&source, "synrepo_search");
    assert!(search_window.contains("Best for exact symbols"));
    assert!(search_window.contains("MCP tool names"));
    assert!(search_window.contains("suggested_card_targets"));

    for tool in ["synrepo_find", "synrepo_where_to_edit"] {
        let window = tool_window(&source, tool);
        assert!(window.contains("Best for plain-language task routing"));
        assert!(window.contains("If the user mentions exact identifiers"));
        assert!(window.contains("call synrepo_search first"));
    }
}

fn tool_window<'a>(source: &'a str, tool: &str) -> &'a str {
    let needle = format!("name = \"{tool}\"");
    let idx = source
        .find(&needle)
        .unwrap_or_else(|| panic!("did not find `{needle}` in MCP tools.rs"));
    let window_end = (idx + 900).min(source.len());
    &source[idx..window_end]
}

fn read_repo_file(parts: &[&str]) -> String {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = parts
        .iter()
        .fold(manifest_dir.to_path_buf(), |path, part| path.join(part));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn assert_required_lines(name: &str, surface: &str, required: &[&str]) {
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|line| !surface.contains(line))
        .collect();
    assert!(
        missing.is_empty(),
        "{name} is missing {} required line(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
}
