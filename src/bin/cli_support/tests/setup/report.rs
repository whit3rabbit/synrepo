use std::fs;

use anyhow::anyhow;
use tempfile::tempdir;

use crate::cli_support::agent_shims::{doctrine::DOCTRINE_BLOCK, AgentTool};
use crate::cli_support::commands::{
    classify_mcp_registration, classify_shim_freshness, entry_after_failure, entry_after_success,
    render_client_setup_summary, ClientBefore, ClientSetupEntry, McpRegistration, ShimFreshness,
};

#[test]
fn report_classifies_current_shim_from_canonical_template() {
    let dir = tempdir().unwrap();
    let path = AgentTool::Claude.output_path(dir.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, AgentTool::Claude.shim_content()).unwrap();

    assert!(
        AgentTool::Claude.shim_content().contains(DOCTRINE_BLOCK),
        "canonical shim must embed the canonical doctrine block"
    );
    assert_eq!(
        classify_shim_freshness(dir.path(), AgentTool::Claude),
        ShimFreshness::Current
    );
}

#[test]
fn report_classifies_stale_shim_and_renders_regen_guidance() {
    let dir = tempdir().unwrap();
    let path = AgentTool::Claude.output_path(dir.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "## Agent doctrine\nold doctrine\n").unwrap();

    let before = ClientBefore::observe(dir.path(), AgentTool::Claude);
    let entry = entry_after_success(dir.path(), AgentTool::Claude, before, false);
    let rendered = render_client_setup_summary(dir.path(), "agent-setup", &[entry]);

    assert_eq!(
        classify_shim_freshness(dir.path(), AgentTool::Claude),
        ShimFreshness::Stale
    );
    assert!(rendered.contains("[stale]"), "{rendered}");
    assert!(
        rendered.contains("synrepo agent-setup claude --regen"),
        "{rendered}"
    );
}

#[test]
fn report_keeps_missing_mcp_registration_separate_from_current_shim() {
    let dir = tempdir().unwrap();
    let path = AgentTool::Claude.output_path(dir.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, AgentTool::Claude.shim_content()).unwrap();

    let before = ClientBefore::observe(dir.path(), AgentTool::Claude);
    let entry = entry_after_success(dir.path(), AgentTool::Claude, before, false);
    let rendered = render_client_setup_summary(dir.path(), "setup", &[entry]);

    assert_eq!(
        classify_mcp_registration(dir.path(), AgentTool::Claude),
        McpRegistration::Missing
    );
    assert!(rendered.contains("[current]"), "{rendered}");
    assert!(
        rendered.contains("mcp: project .mcp.json (missing)"),
        "{rendered}"
    );
    assert!(!rendered.contains("registered"), "{rendered}");
}

#[test]
fn report_renders_skipped_target_output() {
    let dir = tempdir().unwrap();
    let entry = ClientSetupEntry::skipped(dir.path(), AgentTool::Copilot, true);
    let rendered = render_client_setup_summary(dir.path(), "agent-setup", &[entry]);

    assert!(
        rendered.contains("GitHub Copilot [detected, skipped]"),
        "{rendered}"
    );
}

#[test]
fn report_renders_failed_target_output() {
    let dir = tempdir().unwrap();
    let err = anyhow!("blocked path");
    let entry = entry_after_failure(dir.path(), AgentTool::Claude, false, &err);
    let rendered = render_client_setup_summary(dir.path(), "agent-setup", &[entry]);

    assert!(rendered.contains("Claude Code [failed]"), "{rendered}");
    assert!(rendered.contains("error: blocked path"), "{rendered}");
}
