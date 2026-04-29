//! Agent-config backed MCP server registration.

use std::path::Path;

use agent_config::{AgentConfigError, InstallReport, McpSpec, Scope};
use anyhow::Context;

use super::steps::StepOutcome;
use crate::cli_support::agent_shims::{
    scope_label, AgentTool, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER,
};

pub(crate) fn register_synrepo_mcp(
    repo_root: &Path,
    target: AgentTool,
    scope: Scope,
) -> anyhow::Result<StepOutcome> {
    let Some(id) = target.agent_config_id() else {
        return Ok(StepOutcome::NotAutomated);
    };
    let Some(installer) = agent_config::mcp_by_id(id) else {
        return Ok(StepOutcome::NotAutomated);
    };

    let args = match &scope {
        Scope::Global => vec!["mcp".to_string()],
        Scope::Local(_) => vec!["mcp".to_string(), "--repo".to_string(), ".".to_string()],
        _ => vec!["mcp".to_string()],
    };
    let spec = McpSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .stdio("synrepo", args)
        .friendly_name("synrepo")
        .try_build()
        .context("failed to build synrepo MCP installer spec")?;

    let report = installer.install_mcp(&scope, &spec).map_err(|err| match err {
        AgentConfigError::InlineSecretInLocalScope { name, key } => anyhow::anyhow!(
            "agent-config refused MCP install for {id}: server {name:?} includes inline secret key {key:?}"
        ),
        other => anyhow::Error::new(other)
            .context(format!("failed to register synrepo MCP for {}", target.display_name())),
    })?;

    print_install_report(scope_label(&scope), target, &report, repo_root);
    Ok(step_outcome_from_report(&report))
}

pub(crate) fn step_outcome_from_report(report: &InstallReport) -> StepOutcome {
    if report.already_installed {
        StepOutcome::AlreadyCurrent
    } else if !report.patched.is_empty() {
        StepOutcome::Updated
    } else if !report.created.is_empty() {
        StepOutcome::Applied
    } else {
        StepOutcome::AlreadyCurrent
    }
}

fn print_install_report(scope: &str, target: AgentTool, report: &InstallReport, repo_root: &Path) {
    if report.already_installed {
        println!(
            "  synrepo MCP already registered for {} ({scope}, no changes)",
            target.display_name()
        );
        return;
    }

    for path in &report.created {
        println!(
            "  Created {} MCP config for {}: {}",
            scope,
            target.display_name(),
            display_path(path, repo_root)
        );
    }
    for path in &report.patched {
        println!(
            "  Patched {} MCP config for {}: {}",
            scope,
            target.display_name(),
            display_path(path, repo_root)
        );
    }
    for path in &report.backed_up {
        println!("  Backup created: {}", display_path(path, repo_root));
    }
}

fn display_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
