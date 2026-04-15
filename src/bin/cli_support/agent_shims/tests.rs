use super::doctrine::DOCTRINE_BLOCK;
use super::*;

#[test]
fn test_display_name() {
    assert_eq!(AgentTool::Claude.display_name(), "Claude Code");
    assert_eq!(AgentTool::Cursor.display_name(), "Cursor");
    assert_eq!(AgentTool::Copilot.display_name(), "GitHub Copilot");
    assert_eq!(AgentTool::Generic.display_name(), "generic (AGENTS.md)");
    assert_eq!(AgentTool::Codex.display_name(), "Codex CLI");
    assert_eq!(AgentTool::Windsurf.display_name(), "Windsurf");
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
    assert!(DOCTRINE_BLOCK.contains("`require_freshness=true`"));
}
