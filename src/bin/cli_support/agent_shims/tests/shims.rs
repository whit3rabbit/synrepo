use super::*;

#[test]
fn canonical_name_matches_clap_value_enum_form() {
    use clap::ValueEnum;
    for variant in AgentTool::value_variants() {
        let clap_name = variant
            .to_possible_value()
            .expect("AgentTool variants are not skipped")
            .get_name()
            .to_string();
        assert_eq!(
            variant.canonical_name(),
            clap_name.as_str(),
            "canonical_name drift for {variant:?}"
        );
    }
}

#[test]
fn agent_config_ids_are_registered_or_documented_synrepo_only() {
    use clap::ValueEnum;
    let synrepo_only = [AgentTool::Generic, AgentTool::Goose, AgentTool::Kiro];
    for variant in AgentTool::value_variants() {
        match variant.agent_config_id() {
            Some(id) => assert!(
                agent_config::by_id(id).is_some(),
                "{variant:?} maps to unregistered agent-config id {id}"
            ),
            None => assert!(
                synrepo_only.contains(variant),
                "{variant:?} lacks an agent-config id but is not documented synrepo-only"
            ),
        }
    }
}

#[test]
fn automation_tier_tracks_agent_config_mcp_support() {
    use clap::ValueEnum;
    for variant in AgentTool::value_variants() {
        assert_eq!(
            variant.automation_tier(),
            if variant.installer_supports_mcp() {
                AutomationTier::Automated
            } else {
                AutomationTier::ShimOnly
            },
            "automation tier drifted from agent-config MCP registry for {variant:?}"
        );
    }
}

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
    let scope = agent_config::Scope::Local(repo_root.to_path_buf());
    for tool in [
        AgentTool::Claude,
        AgentTool::Cursor,
        AgentTool::Copilot,
        AgentTool::Codex,
        AgentTool::Windsurf,
        AgentTool::OpenCode,
        AgentTool::Gemini,
        AgentTool::Qwen,
        AgentTool::Junie,
        AgentTool::Roo,
        AgentTool::Trae,
    ] {
        assert_eq!(
            tool.output_path(repo_root),
            tool.resolved_shim_output_path(&scope)
                .expect("agent-config backed tool should report a shim path"),
            "{tool:?} output path should come from agent-config status"
        );
    }
    assert_eq!(
        AgentTool::Generic.output_path(repo_root),
        repo_root.join("synrepo-agents.md")
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
        AgentTool::Tabnine.output_path(repo_root),
        repo_root
            .join(".tabnine")
            .join("agent")
            .join("commands")
            .join("synrepo.toml")
    );
}

#[test]
fn test_include_instruction() {
    assert!(AgentTool::Claude
        .include_instruction()
        .contains(".claude/skills/synrepo/SKILL.md"));
    assert!(AgentTool::Cursor
        .include_instruction()
        .contains(".cursor/skills/synrepo/SKILL.md"));
    assert!(AgentTool::Copilot
        .include_instruction()
        .contains("synrepo-copilot-instructions.md"));
    assert!(AgentTool::Generic
        .include_instruction()
        .contains("synrepo-agents.md"));
    assert!(AgentTool::Codex
        .include_instruction()
        .contains(".agents/skills/synrepo/SKILL.md"));
    assert!(AgentTool::Windsurf
        .include_instruction()
        .contains(".windsurf/skills/synrepo/SKILL.md"));
    assert!(AgentTool::OpenCode
        .include_instruction()
        .contains("AGENTS.md"));
    assert!(AgentTool::Gemini
        .include_instruction()
        .contains(".gemini/skills/synrepo/SKILL.md"));
    assert!(AgentTool::Goose
        .include_instruction()
        .contains(".goose/recipes/synrepo.yaml"));
    assert!(AgentTool::Trae
        .include_instruction()
        .contains(".trae/skills/synrepo/SKILL.md"));
}

#[test]
fn test_shim_content_framing() {
    for tool in [
        AgentTool::Claude,
        AgentTool::Cursor,
        AgentTool::Codex,
        AgentTool::Windsurf,
        AgentTool::Gemini,
    ] {
        let body = tool.shim_content();
        assert!(
            body.starts_with("---\nname: synrepo\n"),
            "{} shim does not start with Agent Skills YAML frontmatter",
            tool.display_name()
        );
        assert!(
            body.contains("\ndescription: "),
            "{} shim missing `description:` field",
            tool.display_name()
        );
    }

    assert!(AgentTool::Copilot.shim_content().starts_with("## synrepo"));
    assert!(AgentTool::Generic.shim_content().starts_with("## synrepo"));
    assert!(AgentTool::OpenCode
        .shim_content()
        .starts_with("# synrepo context (OpenCode)"));
}
