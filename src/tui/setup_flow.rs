use std::path::Path;

use crate::bootstrap::runtime_probe::{
    AgentIntegration, AgentTargetKind, ProbeReport, RuntimeClassification,
};
use crate::tui::SetupFlow;

pub(crate) struct SetupWizardSelection {
    pub flow: SetupFlow,
    pub root_gitignore_present: bool,
}

pub(crate) fn default_mode(repo_root: &Path) -> crate::config::Mode {
    if has_concept_directory(repo_root) {
        crate::config::Mode::Curated
    } else {
        crate::config::Mode::Auto
    }
}

/// Return true when a ready repo should show setup follow-up prompts.
pub fn setup_followup_needed(repo_root: &Path, report: &ProbeReport) -> bool {
    if !matches!(report.classification, RuntimeClassification::Ready) {
        return false;
    }
    agent_integration_needs_attention(&report.agent_integration)
        || !root_gitignore_present(repo_root)
        || missing_supported_hooks(repo_root, report)
}

pub(crate) fn select_setup_flow(repo_root: &Path, report: &ProbeReport) -> SetupWizardSelection {
    let root_gitignore_present = root_gitignore_present(repo_root);
    let flow = if matches!(report.classification, RuntimeClassification::Ready)
        && (agent_integration_needs_attention(&report.agent_integration)
            || !root_gitignore_present
            || missing_supported_hooks(repo_root, report))
    {
        SetupFlow::FollowUp
    } else {
        SetupFlow::Full
    };
    SetupWizardSelection {
        flow,
        root_gitignore_present,
    }
}

fn agent_integration_needs_attention(integration: &AgentIntegration) -> bool {
    !matches!(integration, AgentIntegration::Complete { .. })
}

fn missing_supported_hooks(repo_root: &Path, report: &ProbeReport) -> bool {
    hook_candidate(report)
        .is_some_and(|target| target_supports_hooks(target) && !hooks_installed(repo_root, target))
}

fn hook_candidate(report: &ProbeReport) -> Option<AgentTargetKind> {
    report.agent_integration.target().or_else(|| {
        report
            .detected_agent_targets
            .iter()
            .copied()
            .find(|target| target_supports_hooks(*target))
    })
}

fn target_supports_hooks(target: AgentTargetKind) -> bool {
    matches!(target, AgentTargetKind::Claude | AgentTargetKind::Codex)
}

fn hooks_installed(repo_root: &Path, target: AgentTargetKind) -> bool {
    let (path, client) = match target {
        AgentTargetKind::Claude => (repo_root.join(".claude/settings.local.json"), "claude"),
        AgentTargetKind::Codex => (repo_root.join(".codex/hooks.json"), "codex"),
        _ => return false,
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    content.contains(&format!(
        "synrepo agent-hook nudge --client {client} --event UserPromptSubmit"
    )) && content.contains(&format!(
        "synrepo agent-hook nudge --client {client} --event PreToolUse"
    ))
}

fn root_gitignore_present(repo_root: &Path) -> bool {
    crate::bootstrap::root_gitignore_contains_synrepo(repo_root).unwrap_or(false)
}

fn has_concept_directory(repo_root: &Path) -> bool {
    ["docs/concepts", "docs/adr", "docs/decisions"]
        .iter()
        .any(|path| repo_root.join(path).is_dir())
}
