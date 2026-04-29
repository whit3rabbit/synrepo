//! Detection and adoption of unowned legacy synrepo agent installs.
//!
//! `synrepo upgrade` walks every `AgentTool` and asks `agent-config` whether
//! a synrepo MCP / skill / instruction install for the project exists but
//! lacks the ownership ledger entry. Such installs predate the
//! agent-config-managed flow and are adopted in place when the user runs
//! `--apply`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use agent_config::{InstallReport, InstallStatus, Scope, StatusReport};
use clap::ValueEnum;

use crate::cli_support::agent_shims::{
    AgentTool, ShimPlacement, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum LegacyInstallSurface {
    Mcp,
    Skill,
    Instruction {
        placement: agent_config::InstructionPlacement,
    },
}

impl LegacyInstallSurface {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Mcp => "MCP",
            Self::Skill => "skill",
            Self::Instruction { .. } => "instruction",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LegacyAgentInstall {
    pub(super) tool: AgentTool,
    pub(super) surface: LegacyInstallSurface,
    pub(super) path: PathBuf,
}

pub(super) fn detect_legacy_agent_installs(repo_root: &Path) -> Vec<LegacyAgentInstall> {
    let scope = Scope::Local(repo_root.to_path_buf());
    let mut installs = Vec::new();
    let mut seen_mcp_paths = BTreeSet::new();

    for &tool in AgentTool::value_variants() {
        if let Some(id) = tool.agent_config_id() {
            if let Some(mcp) = agent_config::mcp_by_id(id) {
                if let Some(path) = present_unowned_path(mcp.mcp_status(
                    &scope,
                    SYNREPO_INSTALL_NAME,
                    SYNREPO_INSTALL_OWNER,
                ))
                .filter(|path| seen_mcp_paths.insert(path.clone()))
                {
                    installs.push(LegacyAgentInstall {
                        tool,
                        surface: LegacyInstallSurface::Mcp,
                        path,
                    });
                }
            }
        }

        match tool.placement_kind() {
            ShimPlacement::Skill { name } => {
                let Some(id) = tool.agent_config_id() else {
                    continue;
                };
                let Some(skill) = agent_config::skill_by_id(id) else {
                    continue;
                };
                if let Some(path) =
                    present_unowned_path(skill.skill_status(&scope, name, SYNREPO_INSTALL_OWNER))
                {
                    installs.push(LegacyAgentInstall {
                        tool,
                        surface: LegacyInstallSurface::Skill,
                        path,
                    });
                }
            }
            ShimPlacement::Instruction { name, placement } => {
                let Some(id) = tool.agent_config_id() else {
                    continue;
                };
                let Some(instruction) = agent_config::instruction_by_id(id) else {
                    continue;
                };
                if let Some(path) = present_unowned_path(instruction.instruction_status(
                    &scope,
                    name,
                    SYNREPO_INSTALL_OWNER,
                )) {
                    installs.push(LegacyAgentInstall {
                        tool,
                        surface: LegacyInstallSurface::Instruction { placement },
                        path,
                    });
                }
            }
            ShimPlacement::Local => {}
        }
    }

    installs
}

fn present_unowned_path(
    report: Result<StatusReport, agent_config::AgentConfigError>,
) -> Option<PathBuf> {
    let report = report.ok()?;
    if report.status != InstallStatus::PresentUnowned {
        return None;
    }
    report.config_path.or_else(|| {
        report.files.into_iter().find_map(|status| match status {
            agent_config::PathStatus::Exists { path }
            | agent_config::PathStatus::Missing { path }
            | agent_config::PathStatus::Invalid { path, .. } => Some(path),
            _ => None,
        })
    })
}

pub(super) fn apply_legacy_agent_installs(
    repo_root: &Path,
    installs: &[LegacyAgentInstall],
) -> anyhow::Result<()> {
    if installs.is_empty() {
        return Ok(());
    }
    let scope = Scope::Local(repo_root.to_path_buf());
    for install in installs {
        let report = match install.surface {
            LegacyInstallSurface::Mcp => adopt_legacy_mcp(install.tool, &scope)?,
            LegacyInstallSurface::Skill => adopt_legacy_skill(install.tool, &scope)?,
            LegacyInstallSurface::Instruction { placement } => {
                adopt_legacy_instruction(install.tool, &scope, placement)?
            }
        };
        print_legacy_install_report(install, &report);
    }
    Ok(())
}

fn require_agent_config_id(tool: AgentTool) -> anyhow::Result<&'static str> {
    tool.agent_config_id().ok_or_else(|| {
        anyhow::anyhow!("{} has no agent-config integration id", tool.display_name())
    })
}

fn require_installer<T>(installer: Option<T>, tool: AgentTool, surface: &str) -> anyhow::Result<T> {
    installer.ok_or_else(|| {
        anyhow::anyhow!(
            "{} does not support agent-config {surface}",
            tool.display_name()
        )
    })
}

fn adopt_legacy_mcp(tool: AgentTool, scope: &Scope) -> anyhow::Result<InstallReport> {
    let id = require_agent_config_id(tool)?;
    let installer = require_installer(agent_config::mcp_by_id(id), tool, "MCP")?;
    let spec = agent_config::McpSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .stdio("synrepo", ["mcp", "--repo", "."])
        .friendly_name("synrepo")
        .adopt_unowned(true)
        .try_build()?;
    installer
        .install_mcp(scope, &spec)
        .map_err(anyhow::Error::new)
}

fn adopt_legacy_skill(tool: AgentTool, scope: &Scope) -> anyhow::Result<InstallReport> {
    let id = require_agent_config_id(tool)?;
    let installer = require_installer(agent_config::skill_by_id(id), tool, "skills")?;
    let spec = agent_config::SkillSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .description(tool.skill_description())
        .body(tool.shim_spec_body())
        .adopt_unowned(true)
        .try_build()?;
    installer
        .install_skill(scope, &spec)
        .map_err(anyhow::Error::new)
}

fn adopt_legacy_instruction(
    tool: AgentTool,
    scope: &Scope,
    placement: agent_config::InstructionPlacement,
) -> anyhow::Result<InstallReport> {
    let id = require_agent_config_id(tool)?;
    let installer = require_installer(agent_config::instruction_by_id(id), tool, "instructions")?;
    let spec = agent_config::InstructionSpec::builder(SYNREPO_INSTALL_NAME)
        .owner(SYNREPO_INSTALL_OWNER)
        .placement(placement)
        .body(tool.shim_spec_body())
        .adopt_unowned(true)
        .try_build()?;
    installer
        .install_instruction(scope, &spec)
        .map_err(anyhow::Error::new)
}

fn print_legacy_install_report(install: &LegacyAgentInstall, report: &InstallReport) {
    if report.already_installed {
        println!(
            "  adopted legacy {} install for {}: already current",
            install.surface.as_str(),
            install.tool.canonical_name()
        );
        return;
    }
    for path in &report.created {
        println!("  created {}", path.display());
    }
    for path in &report.patched {
        println!("  patched {}", path.display());
    }
    for path in &report.backed_up {
        println!("  backup created {}", path.display());
    }
}
