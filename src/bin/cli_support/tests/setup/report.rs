use std::fs;

use anyhow::anyhow;
use tempfile::tempdir;

use crate::cli_support::agent_shims::{doctrine::DOCTRINE_BLOCK, AgentTool};
use crate::cli_support::commands::{
    classify_mcp_registration, classify_shim_freshness, entry_after_failure, entry_after_success,
    render_client_setup_summary, ClientBefore, ClientSetupEntry, McpRegistration, ShimFreshness,
};

fn local_scope(repo: &std::path::Path) -> agent_config::Scope {
    agent_config::Scope::Local(repo.to_path_buf())
}

#[test]
fn report_classifies_current_shim_from_canonical_template() {
    let dir = tempdir().unwrap();
    let scope = local_scope(dir.path());
    let path = AgentTool::Claude.output_path(dir.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, AgentTool::Claude.shim_content()).unwrap();

    assert!(
        AgentTool::Claude.shim_content().contains(DOCTRINE_BLOCK),
        "canonical shim must embed the canonical doctrine block"
    );
    assert_eq!(
        classify_shim_freshness(dir.path(), AgentTool::Claude, &scope),
        ShimFreshness::Current
    );
}

#[test]
fn report_classifies_stale_shim_and_renders_regen_guidance() {
    let dir = tempdir().unwrap();
    let scope = local_scope(dir.path());
    let path = AgentTool::Claude.output_path(dir.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "## Agent doctrine\nold doctrine\n").unwrap();

    let before = ClientBefore::observe(dir.path(), AgentTool::Claude, &scope);
    let entry = entry_after_success(dir.path(), AgentTool::Claude, before, false, &scope);
    let rendered = render_client_setup_summary(dir.path(), "agent-setup", &[entry]);

    assert_eq!(
        classify_shim_freshness(dir.path(), AgentTool::Claude, &scope),
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
    let scope = local_scope(dir.path());
    let path = AgentTool::Claude.output_path(dir.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, AgentTool::Claude.shim_content()).unwrap();

    let before = ClientBefore::observe(dir.path(), AgentTool::Claude, &scope);
    let entry = entry_after_success(dir.path(), AgentTool::Claude, before, false, &scope);
    let rendered = render_client_setup_summary(dir.path(), "setup", &[entry]);

    assert_eq!(
        classify_mcp_registration(dir.path(), AgentTool::Claude, &scope),
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
fn report_classifies_codex_mcp_servers_schema_only() {
    let dir = tempdir().unwrap();
    let scope = local_scope(dir.path());
    let spec = agent_config::McpSpec::builder("synrepo")
        .owner("synrepo")
        .stdio("synrepo", ["mcp", "--repo", "."])
        .build();
    let _ = agent_config::mcp_by_id("codex")
        .unwrap()
        .install_mcp(&scope, &spec)
        .unwrap();

    assert_eq!(
        classify_mcp_registration(dir.path(), AgentTool::Codex, &scope),
        McpRegistration::Registered
    );

    fs::write(
        dir.path().join(".codex").join("config.toml"),
        "[mcp]\nsynrepo = \"synrepo mcp --repo .\"\n",
    )
    .unwrap();
    assert_eq!(
        classify_mcp_registration(dir.path(), AgentTool::Codex, &scope),
        McpRegistration::Missing,
        "legacy [mcp].synrepo must not be reported as current Codex MCP"
    );
}

#[test]
fn report_renders_skipped_target_output() {
    let dir = tempdir().unwrap();
    let scope = local_scope(dir.path());
    let entry = ClientSetupEntry::skipped(dir.path(), AgentTool::Copilot, true, &scope);
    let rendered = render_client_setup_summary(dir.path(), "agent-setup", &[entry]);

    assert!(
        rendered.contains("GitHub Copilot [detected, skipped]"),
        "{rendered}"
    );
}

#[test]
fn report_renders_failed_target_output() {
    let dir = tempdir().unwrap();
    let scope = local_scope(dir.path());
    let err = anyhow!("blocked path");
    let entry = entry_after_failure(dir.path(), AgentTool::Claude, false, &scope, &err);
    let rendered = render_client_setup_summary(dir.path(), "agent-setup", &[entry]);

    assert!(rendered.contains("Claude Code [failed]"), "{rendered}");
    assert!(rendered.contains("error: blocked path"), "{rendered}");
}
