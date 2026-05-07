//! Agent-config backed MCP server registration.

use std::path::Path;

use agent_config::{AgentConfigError, InstallReport, McpSpec, PlannedChange, Scope, StatusReport};
use anyhow::Context;

use super::steps::StepOutcome;
use crate::cli_support::agent_shims::{
    scope_label, AgentTool, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER,
};

pub(crate) fn register_synrepo_mcp(
    repo_root: &Path,
    target: AgentTool,
    scope: Scope,
    force_adopt_unowned: bool,
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
    let spec = build_synrepo_mcp_spec(args.clone(), false)?;

    let report = match installer.install_mcp(&scope, &spec) {
        Ok(report) => report,
        Err(AgentConfigError::NotOwnedByCaller { actual: None, .. }) => {
            let adopt_spec = build_synrepo_mcp_spec(args, true)?;
            if force_adopt_unowned
                || adoption_is_ledger_only(installer.as_ref(), &scope, &adopt_spec)?
            {
                installer
                    .install_mcp(&scope, &adopt_spec)
                    .map_err(|err| installer_error(id, target, &scope, err))?
            } else {
                let status = installer
                    .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
                    .ok();
                return Err(unowned_synrepo_error(repo_root, target, &scope, status));
            }
        }
        Err(err) => return Err(installer_error(id, target, &scope, err)),
    };

    print_install_report(scope_label(&scope), target, &report, repo_root);
    Ok(step_outcome_from_report(&report))
}

fn build_synrepo_mcp_spec(args: Vec<String>, adopt_unowned: bool) -> anyhow::Result<McpSpec> {
    McpSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .stdio("synrepo", args)
        .friendly_name("synrepo")
        .adopt_unowned(adopt_unowned)
        .try_build()
        .context("failed to build synrepo MCP installer spec")
}

fn adoption_is_ledger_only(
    installer: &dyn agent_config::McpSurface,
    scope: &Scope,
    spec: &McpSpec,
) -> anyhow::Result<bool> {
    let plan = installer
        .plan_install_mcp(scope, spec)
        .context("failed to preflight synrepo MCP adoption")?;
    Ok(plan.changes.iter().all(|change| {
        matches!(
            change,
            PlannedChange::WriteLedger { .. } | PlannedChange::NoOp { .. }
        )
    }))
}

fn installer_error(
    id: &str,
    target: AgentTool,
    scope: &Scope,
    err: AgentConfigError,
) -> anyhow::Error {
    match err {
        AgentConfigError::InlineSecretInLocalScope { name, key } => anyhow::anyhow!(
            "agent-config refused MCP install for {id}: server {name:?} includes inline secret key {key:?}"
        ),
        AgentConfigError::NotOwnedByCaller { actual: None, .. } => {
            unowned_synrepo_error(Path::new("."), target, scope, None)
        }
        other => anyhow::Error::new(other)
            .context(format!("failed to register synrepo MCP for {}", target.display_name())),
    }
}

fn unowned_synrepo_error(
    repo_root: &Path,
    target: AgentTool,
    scope: &Scope,
    status: Option<StatusReport>,
) -> anyhow::Error {
    let config = status
        .and_then(|report| report.config_path)
        .map(|path| display_path(&path, repo_root))
        .unwrap_or_else(|| "the target MCP config".to_string());
    let scope_flag = if matches!(scope, Scope::Local(_)) {
        " --project"
    } else {
        ""
    };
    anyhow::anyhow!(
        "failed to register synrepo MCP for {client}: existing `synrepo` MCP entry in {config} is unowned by agent-config and does not match the desired synrepo launcher. Inspect or remove that entry manually, or run `synrepo setup {tool}{scope_flag} --force` to replace and adopt it.",
        client = target.display_name(),
        tool = target.canonical_name(),
    )
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
    if report.created.is_empty() && report.patched.is_empty() && report.backed_up.is_empty() {
        println!(
            "  synrepo MCP already registered for {} ({scope}, ownership adopted)",
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
