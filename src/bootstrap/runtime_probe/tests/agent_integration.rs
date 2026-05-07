use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn agent_integration_codex_mcp_only_reports_shim_missing() {
    let dir = tempdir().unwrap();
    let codex = dir.path().join(".codex");
    fs::create_dir_all(&codex).unwrap();
    fs::write(
        codex.join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n",
    )
    .unwrap();
    fs::write(
        codex.join(".agent-config-mcp.json"),
        r#"{"version":2,"entries":{"synrepo":{"owner":"synrepo","content_hash":"test"}}}"#,
    )
    .unwrap();

    let report = probe_with_home(dir.path(), None);
    assert_eq!(
        report.agent_integration,
        AgentIntegration::McpOnly {
            target: AgentTargetKind::Codex
        }
    );
}
