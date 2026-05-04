use tempfile::tempdir;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::{step_install_agent_hooks, StepOutcome};

#[test]
fn step_install_agent_hooks_writes_codex_and_claude_local_configs() {
    let dir = tempdir().unwrap();

    let codex_first = step_install_agent_hooks(dir.path(), AgentTool::Codex).unwrap();
    let claude_first = step_install_agent_hooks(dir.path(), AgentTool::Claude).unwrap();
    let codex_second = step_install_agent_hooks(dir.path(), AgentTool::Codex).unwrap();

    assert_eq!(codex_first, StepOutcome::Applied);
    assert_eq!(claude_first, StepOutcome::Applied);
    assert_eq!(codex_second, StepOutcome::AlreadyCurrent);
    assert!(dir.path().join(".codex/hooks.json").exists());
    assert!(dir.path().join(".claude/settings.local.json").exists());
    assert!(
        !dir.path().join(".codex/config.toml").exists(),
        "hook install must preserve MCP config by not touching it"
    );
}

#[test]
fn step_install_agent_hooks_rejects_unsupported_targets() {
    let dir = tempdir().unwrap();
    let err = step_install_agent_hooks(dir.path(), AgentTool::Cursor).unwrap_err();
    assert!(
        err.to_string().contains("does not support"),
        "unexpected error: {err:#}"
    );
}
