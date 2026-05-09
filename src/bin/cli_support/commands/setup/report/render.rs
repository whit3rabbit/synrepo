use std::path::Path;

use crate::cli_support::agent_shims::AgentTool;

use super::codex_warnings::{codex_global_skill_warnings, codex_skill_warning_reason};
use super::{ClientSetupEntry, ShimFreshness};

pub(crate) fn render_detected_client_summary(
    detected: &[AgentTool],
    selected: &[AgentTool],
    skipped: &[AgentTool],
) -> String {
    let mut out = String::new();
    out.push_str("Detected clients: ");
    out.push_str(&render_tool_list(detected));
    out.push('\n');
    out.push_str("Selected clients: ");
    out.push_str(&render_tool_list(selected));
    out.push('\n');
    if !skipped.is_empty() {
        out.push_str("Skipped clients: ");
        out.push_str(&render_tool_list(skipped));
        out.push('\n');
    }
    out
}

pub(crate) fn render_client_setup_summary(
    repo_root: &Path,
    kind: &str,
    entries: &[ClientSetupEntry],
) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = format!("Client {kind} summary:\n");
    for entry in entries {
        let outcomes = entry
            .outcomes
            .iter()
            .map(|outcome| outcome.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "  - {} [{}]\n",
            entry.tool.display_name(),
            outcomes
        ));
        out.push_str(&format!(
            "    shim: {} ({})\n",
            entry.shim_path.render(repo_root),
            entry.shim.as_str()
        ));
        match &entry.mcp_path {
            Some(path) => out.push_str(&format!(
                "    mcp: {} ({})\n",
                path.render(repo_root),
                entry.mcp.as_str()
            )),
            None => out.push_str(&format!("    mcp: {}\n", entry.mcp.as_str())),
        }
        if entry.shim == ShimFreshness::Stale {
            out.push_str(&format!(
                "    next: run `synrepo agent-setup {} --regen` to refresh the shim\n",
                entry.tool.canonical_name()
            ));
        }
        if entry.tool == AgentTool::Codex {
            for warning in codex_global_skill_warnings() {
                out.push_str(&format!(
                    "    warning: global Codex skill {} {}.\n",
                    warning.path.display(),
                    codex_skill_warning_reason(&warning)
                ));
                out.push_str(
                    "    next: run `synrepo setup codex --force` for global setup, or `synrepo agent-setup codex --regen` for a project-local refresh\n",
                );
            }
        }
        if let Some(error) = &entry.error {
            out.push_str(&format!("    error: {error}\n"));
        }
    }
    out
}

fn render_tool_list(tools: &[AgentTool]) -> String {
    if tools.is_empty() {
        "none".to_string()
    } else {
        tools
            .iter()
            .map(|tool| tool.canonical_name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
