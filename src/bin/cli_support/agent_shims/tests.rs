use super::doctrine::{DOCTRINE_BLOCK, TOOL_DESC_ESCALATION_LINE};
use super::*;

#[test]
fn test_display_name() {
    assert_eq!(AgentTool::Claude.display_name(), "Claude Code");
    assert_eq!(AgentTool::Cursor.display_name(), "Cursor");
    assert_eq!(AgentTool::Copilot.display_name(), "GitHub Copilot");
    assert_eq!(AgentTool::Generic.display_name(), "generic (AGENTS.md)");
    assert_eq!(AgentTool::Codex.display_name(), "Codex CLI");
    assert_eq!(AgentTool::Windsurf.display_name(), "Windsurf");
    assert_eq!(AgentTool::OpenCode.display_name(), "OpenCode");
    assert_eq!(AgentTool::Gemini.display_name(), "Gemini CLI");
    assert_eq!(AgentTool::Goose.display_name(), "Goose");
    assert_eq!(AgentTool::Kiro.display_name(), "Kiro CLI");
    assert_eq!(AgentTool::Qwen.display_name(), "Qwen Code");
    assert_eq!(AgentTool::Junie.display_name(), "Junie");
    assert_eq!(AgentTool::Roo.display_name(), "Roo Code");
    assert_eq!(AgentTool::Tabnine.display_name(), "Tabnine CLI");
    assert_eq!(AgentTool::Trae.display_name(), "Trae");
}

#[test]
fn test_output_path() {
    let repo_root = std::path::Path::new("/mock/repo");
    assert_eq!(
        AgentTool::Claude.output_path(repo_root),
        repo_root.join(".claude").join("synrepo-context.md")
    );
    assert_eq!(
        AgentTool::Cursor.output_path(repo_root),
        repo_root.join(".cursor").join("synrepo.mdc")
    );
    assert_eq!(
        AgentTool::Copilot.output_path(repo_root),
        repo_root.join("synrepo-copilot-instructions.md")
    );
    assert_eq!(
        AgentTool::Generic.output_path(repo_root),
        repo_root.join("synrepo-agents.md")
    );
    assert_eq!(
        AgentTool::Codex.output_path(repo_root),
        repo_root.join(".codex").join("instructions.md")
    );
    assert_eq!(
        AgentTool::Windsurf.output_path(repo_root),
        repo_root.join(".windsurf").join("rules").join("synrepo.md")
    );
    assert_eq!(
        AgentTool::OpenCode.output_path(repo_root),
        repo_root.join("AGENTS.md")
    );
    assert_eq!(
        AgentTool::Gemini.output_path(repo_root),
        repo_root
            .join(".gemini")
            .join("commands")
            .join("synrepo.toml")
    );
    assert_eq!(
        AgentTool::Goose.output_path(repo_root),
        repo_root
            .join(".goose")
            .join("recipes")
            .join("synrepo.yaml")
    );
    assert_eq!(
        AgentTool::Kiro.output_path(repo_root),
        repo_root.join(".kiro").join("prompts").join("synrepo.md")
    );
    assert_eq!(
        AgentTool::Qwen.output_path(repo_root),
        repo_root.join(".qwen").join("commands").join("synrepo.md")
    );
    assert_eq!(
        AgentTool::Junie.output_path(repo_root),
        repo_root.join(".junie").join("commands").join("synrepo.md")
    );
    assert_eq!(
        AgentTool::Roo.output_path(repo_root),
        repo_root.join(".roo").join("commands").join("synrepo.md")
    );
    assert_eq!(
        AgentTool::Tabnine.output_path(repo_root),
        repo_root
            .join(".tabnine")
            .join("agent")
            .join("commands")
            .join("synrepo.toml")
    );
    assert_eq!(
        AgentTool::Trae.output_path(repo_root),
        repo_root
            .join(".trae")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md")
    );
}

#[test]
fn test_include_instruction() {
    assert!(AgentTool::Claude
        .include_instruction()
        .contains("synrepo-context.md"));
    assert!(AgentTool::Cursor
        .include_instruction()
        .contains("synrepo.mdc"));
    assert!(AgentTool::Copilot
        .include_instruction()
        .contains("synrepo-copilot-instructions.md"));
    assert!(AgentTool::Generic
        .include_instruction()
        .contains("synrepo-agents.md"));
    assert!(AgentTool::Codex
        .include_instruction()
        .contains(".codex/instructions.md"));
    assert!(AgentTool::Windsurf
        .include_instruction()
        .contains(".windsurf/rules/synrepo.md"));
    assert!(AgentTool::OpenCode
        .include_instruction()
        .contains("AGENTS.md"));
    // New targets have manual instructions
    assert!(AgentTool::Gemini
        .include_instruction()
        .contains(".gemini/commands/synrepo.toml"));
    assert!(AgentTool::Goose
        .include_instruction()
        .contains(".goose/recipes/synrepo.yaml"));
    assert!(AgentTool::Trae
        .include_instruction()
        .contains(".trae/skills/synrepo/SKILL.md"));
}

#[test]
fn test_shim_content_framing() {
    assert!(AgentTool::Claude
        .shim_content()
        .starts_with("# synrepo context"));
    assert!(AgentTool::Cursor
        .shim_content()
        .starts_with("---\ndescription"));
    assert!(AgentTool::Copilot.shim_content().starts_with("## synrepo"));
    assert!(AgentTool::Generic.shim_content().starts_with("## synrepo"));
    assert!(AgentTool::Codex
        .shim_content()
        .starts_with("# synrepo context"));
    assert!(AgentTool::Windsurf
        .shim_content()
        .starts_with("# synrepo\n"));
    assert!(AgentTool::OpenCode
        .shim_content()
        .starts_with("# synrepo context (OpenCode)"));
}

/// Guard against runaway edits to the doctrine block.
#[test]
fn doctrine_block_size_is_bounded() {
    assert!(!DOCTRINE_BLOCK.is_empty());
    assert!(
        DOCTRINE_BLOCK.len() < 4096,
        "DOCTRINE_BLOCK grew past 4 KiB ({}); consider whether the shim still fits its purpose",
        DOCTRINE_BLOCK.len()
    );
}

/// Every shim MUST contain the canonical doctrine block verbatim. This is the
/// byte-identical guarantee.
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
            tool.shim_content().contains(DOCTRINE_BLOCK),
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
}

/// SKILL.md duplicates doctrine prose (Markdown rendered tools cannot embed a
/// Rust constant). Assert the load-bearing lines from `DOCTRINE_BLOCK` appear
/// verbatim so SKILL.md cannot drift away from the canonical doctrine.
#[test]
fn skill_md_includes_doctrine_lines_verbatim() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let skill_path = manifest_dir.join("skill").join("SKILL.md");
    let skill = std::fs::read_to_string(&skill_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", skill_path.display()));

    let required = [
        // Default path bullets
        "Use `tiny` cards to orient and route.",
        "Use `normal` cards to understand a neighborhood.",
        "Use `deep` cards only before writing code, or when exact source or body details matter.",
        // Do-not bullets
        "Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.",
        "Do not treat overlay commentary as canonical. It is advisory prose layered on structural cards.",
        "Do not trigger synthesis (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.",
        "Do not expect watch or background behavior unless `synrepo watch` is explicitly running.",
        // Product-boundary bullets
        "synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.",
        "Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.",
        "Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.",
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

/// rmcp's `#[tool]` attribute rejects `concat!()` in `description`, so each
/// card-returning tool description duplicates the escalation sentence. This
/// source-scan test detects drift: if a card-returning tool's description is
/// edited and the escalation sentence is dropped or reworded, the test fails.
#[test]
fn card_returning_mcp_tool_descriptions_share_escalation_line() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let main_path = manifest_dir
        .join("src")
        .join("bin")
        .join("cli_support")
        .join("commands")
        .join("mcp.rs");
    let source = std::fs::read_to_string(&main_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", main_path.display()));

    // Card-returning tools per openspec/changes/agent-doctrine-v1/specs/mcp-surface/spec.md.
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
        // Each #[tool(...)] attribute spans one or two lines; the description
        // sits within ~8 lines of the `name` field.
        let window_end = (idx + 800).min(source.len());
        let window = &source[idx..window_end];
        assert!(
            window.contains(TOOL_DESC_ESCALATION_LINE),
            "card-returning MCP tool `{tool}` description does not contain TOOL_DESC_ESCALATION_LINE"
        );
    }

    // Non-card tools must NOT carry the escalation sentence (their default-
    // budget semantics differ).
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
