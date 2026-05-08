use std::path::{Path, PathBuf};

use agent_config::{InstallStatus, Scope, ScopeKind};

use crate::agent_install::{skill_manifest_path, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::registry::AgentEntry;

use super::{
    registry_scope, resolve_registry_path, ComponentKind, ComponentStatus, InstallScope,
    IntegrationComponent,
};

pub(super) fn resolve_context(
    repo_root: &Path,
    tool: &str,
    registry_agent: Option<&AgentEntry>,
) -> IntegrationComponent {
    if let Some(skill) = agent_config::skill_by_id(tool) {
        if let Some(component) = resolve_skill(repo_root, skill.as_ref()) {
            return component;
        }
        if let Some(component) = registry_context(repo_root, registry_agent, ComponentKind::Skill) {
            return component;
        }
        return missing_context(repo_root, skill.as_ref(), ComponentKind::Skill);
    }

    if let Some(instruction) = agent_config::instruction_by_id(tool) {
        if let Some(component) = resolve_instruction(repo_root, instruction.as_ref()) {
            return component;
        }
        if let Some(component) =
            registry_context(repo_root, registry_agent, ComponentKind::Instructions)
        {
            return component;
        }
        return missing_instruction(repo_root, instruction.as_ref());
    }

    registry_context(repo_root, registry_agent, ComponentKind::Instructions).unwrap_or_else(|| {
        IntegrationComponent {
            kind: ComponentKind::Unsupported,
            status: ComponentStatus::Unsupported,
            scope: InstallScope::Unsupported,
            path: None,
            source: "unsupported".to_string(),
        }
    })
}

fn resolve_skill(
    repo_root: &Path,
    skill: &dyn agent_config::SkillSurface,
) -> Option<IntegrationComponent> {
    for (scope, label) in scopes(repo_root, skill.supported_skill_scopes()) {
        let report = skill
            .skill_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .ok()?;
        if installed(&report.status) {
            let source = source_label(&report.status).to_string();
            return Some(IntegrationComponent {
                kind: ComponentKind::Skill,
                status: ComponentStatus::Installed,
                scope: label,
                path: skill_manifest_path(report),
                source,
            });
        }
    }
    None
}

fn missing_context(
    repo_root: &Path,
    skill: &dyn agent_config::SkillSurface,
    kind: ComponentKind,
) -> IntegrationComponent {
    let path = first_skill_path(repo_root, skill);
    IntegrationComponent {
        kind,
        status: ComponentStatus::Missing,
        scope: InstallScope::Missing,
        path,
        source: "not installed".to_string(),
    }
}

fn first_skill_path(repo_root: &Path, skill: &dyn agent_config::SkillSurface) -> Option<PathBuf> {
    scopes(repo_root, skill.supported_skill_scopes())
        .into_iter()
        .find_map(|(scope, _)| {
            skill
                .skill_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
                .ok()
                .and_then(skill_manifest_path)
        })
}

fn resolve_instruction(
    repo_root: &Path,
    instruction: &dyn agent_config::InstructionSurface,
) -> Option<IntegrationComponent> {
    for (scope, label) in scopes(repo_root, instruction.supported_instruction_scopes()) {
        let report = instruction
            .instruction_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .ok()?;
        if installed(&report.status) {
            return Some(IntegrationComponent {
                kind: ComponentKind::Instructions,
                status: ComponentStatus::Installed,
                scope: label,
                path: report.config_path,
                source: source_label(&report.status).to_string(),
            });
        }
    }
    None
}

fn missing_instruction(
    repo_root: &Path,
    instruction: &dyn agent_config::InstructionSurface,
) -> IntegrationComponent {
    let path = scopes(repo_root, instruction.supported_instruction_scopes())
        .into_iter()
        .find_map(|(scope, _)| {
            instruction
                .instruction_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
                .ok()
                .and_then(|report| report.config_path)
        });
    IntegrationComponent {
        kind: ComponentKind::Instructions,
        status: ComponentStatus::Missing,
        scope: InstallScope::Missing,
        path,
        source: "not installed".to_string(),
    }
}

fn registry_context(
    repo_root: &Path,
    registry_agent: Option<&AgentEntry>,
    fallback_kind: ComponentKind,
) -> Option<IntegrationComponent> {
    let agent = registry_agent?;
    if agent.shim_path.is_empty() {
        return None;
    }
    let path = resolve_registry_path(repo_root, &agent.shim_path);
    let kind = if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
        ComponentKind::Skill
    } else {
        fallback_kind
    };
    let status = if path.exists() {
        ComponentStatus::Installed
    } else {
        ComponentStatus::Missing
    };
    Some(IntegrationComponent {
        kind,
        status,
        scope: registry_scope(&agent.scope),
        path: Some(path),
        source: "registry record".to_string(),
    })
}

fn scopes(repo_root: &Path, supported: &[ScopeKind]) -> Vec<(Scope, InstallScope)> {
    let mut out = Vec::new();
    if supported.contains(&ScopeKind::Local) {
        out.push((Scope::Local(repo_root.to_path_buf()), InstallScope::Project));
    }
    if supported.contains(&ScopeKind::Global) {
        out.push((Scope::Global, InstallScope::Global));
    }
    out
}

fn installed(status: &InstallStatus) -> bool {
    matches!(
        status,
        InstallStatus::InstalledOwned { .. }
            | InstallStatus::PresentUnowned
            | InstallStatus::InstalledOtherOwner { .. }
    )
}

fn source_label(status: &InstallStatus) -> &'static str {
    match status {
        InstallStatus::InstalledOwned { .. } => "agent-config owned",
        InstallStatus::PresentUnowned => "legacy config",
        InstallStatus::InstalledOtherOwner { .. } => "agent-config other owner",
        _ => "not installed",
    }
}
