//! Agent-integration detection functions.

use std::path::{Path, PathBuf};

use agent_config::{InstallStatus, Scope, ScopeKind};

use crate::agent_install::{skill_manifest_path, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};

use super::types::{AgentIntegration, AgentTargetKind};

/// Detect agent integration state for the given repo.
pub fn detect_agent_integration(
    repo_root: &Path,
    _synrepo_dir: &Path,
    _config: Option<&crate::config::Config>,
    detected_targets: &[AgentTargetKind],
) -> AgentIntegration {
    // Choose the target to report on: prefer the first detected hint, else
    // walk known targets in a stable order looking for any shim file.
    let probe_order: Vec<AgentTargetKind> = if detected_targets.is_empty() {
        all_agent_targets().to_vec()
    } else {
        detected_targets.to_vec()
    };

    let mut best = AgentIntegration::Absent;
    for target in probe_order {
        let shim = shim_exists(repo_root, target);
        let mcp = mcp_registration_present(repo_root, target);
        let current = match (shim, mcp) {
            (true, true) => AgentIntegration::Complete { target },
            (true, false) => AgentIntegration::Partial { target },
            (false, true) => AgentIntegration::McpOnly { target },
            (false, false) => continue,
        };
        if matches!(current, AgentIntegration::Complete { .. }) {
            return current;
        }
        if matches!(best, AgentIntegration::Absent) {
            best = current;
        }
    }
    best
}

/// All known agent target kinds in stable order.
pub fn all_agent_targets() -> &'static [AgentTargetKind] {
    &[
        AgentTargetKind::Claude,
        AgentTargetKind::Cursor,
        AgentTargetKind::Codex,
        AgentTargetKind::Copilot,
        AgentTargetKind::Windsurf,
        AgentTargetKind::Amp,
        AgentTargetKind::Antigravity,
        AgentTargetKind::Cline,
        AgentTargetKind::CodeBuddy,
        AgentTargetKind::Crush,
        AgentTargetKind::Forge,
        AgentTargetKind::Gemini,
        AgentTargetKind::Hermes,
        AgentTargetKind::Iflow,
        AgentTargetKind::Junie,
        AgentTargetKind::Kilocode,
        AgentTargetKind::Opencode,
        AgentTargetKind::Openclaw,
        AgentTargetKind::Pi,
        AgentTargetKind::Qodercli,
        AgentTargetKind::Qwen,
        AgentTargetKind::Roo,
        AgentTargetKind::Tabnine,
        AgentTargetKind::Trae,
    ]
}

fn shim_exists(repo_root: &Path, target: AgentTargetKind) -> bool {
    context_registration_present(repo_root, target) || shim_output_path(repo_root, target).exists()
}

/// Path to the shim output file for the given target.
pub fn shim_output_path(repo_root: &Path, target: AgentTargetKind) -> PathBuf {
    let scope = agent_config::Scope::Local(repo_root.to_path_buf());
    if let Some(path) = agent_config_shim_output_path(&scope, target) {
        return path;
    }
    legacy_shim_output_path(repo_root, target)
}

fn agent_config_shim_output_path(
    scope: &agent_config::Scope,
    target: AgentTargetKind,
) -> Option<PathBuf> {
    let id = agent_config_id(target);
    if let Some(skill) = agent_config::skill_by_id(id) {
        let report = skill
            .skill_status(scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .ok()?;
        return skill_manifest_path(report);
    }
    if let Some(instruction) = agent_config::instruction_by_id(id) {
        return instruction
            .instruction_status(scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .ok()
            .and_then(|report| report.config_path);
    }
    None
}

fn context_registration_present(repo_root: &Path, target: AgentTargetKind) -> bool {
    let id = agent_config_id(target);
    if let Some(skill) = agent_config::skill_by_id(id) {
        for scope in scopes(repo_root, skill.supported_skill_scopes()) {
            if let Ok(report) =
                skill.skill_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            {
                if installed(&report.status) {
                    return true;
                }
            }
        }
    }
    if let Some(instruction) = agent_config::instruction_by_id(id) {
        for scope in scopes(repo_root, instruction.supported_instruction_scopes()) {
            if let Ok(report) =
                instruction.instruction_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            {
                if installed(&report.status) {
                    return true;
                }
            }
        }
    }
    false
}

fn agent_config_id(target: AgentTargetKind) -> &'static str {
    target.as_str()
}

fn legacy_shim_output_path(repo_root: &Path, target: AgentTargetKind) -> PathBuf {
    match target {
        AgentTargetKind::Claude => repo_root
            .join(".claude")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        AgentTargetKind::Cursor => repo_root
            .join(".cursor")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        AgentTargetKind::Codex => repo_root
            .join(".agents")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        AgentTargetKind::Copilot => repo_root.join("synrepo-copilot-instructions.md"),
        AgentTargetKind::Windsurf => repo_root
            .join(".windsurf")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        target => repo_root
            .join(format!(".{}", target.as_str()))
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
    }
}

fn mcp_registration_present(repo_root: &Path, target: AgentTargetKind) -> bool {
    let id = target.as_str();
    if let Some(installer) = agent_config::mcp_by_id(id) {
        let mut checked = false;
        for scope in scopes(repo_root, installer.supported_mcp_scopes()) {
            if let Ok(report) =
                installer.mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            {
                checked = true;
                if installed(&report.status) {
                    return true;
                }
            }
        }
        if checked {
            return false;
        }
    }
    shim_exists(repo_root, target)
}

fn scopes(repo_root: &Path, supported: &[ScopeKind]) -> Vec<Scope> {
    let mut out = Vec::new();
    if supported.contains(&ScopeKind::Local) {
        out.push(Scope::Local(repo_root.to_path_buf()));
    }
    if supported.contains(&ScopeKind::Global) {
        out.push(Scope::Global);
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

/// Detect agent targets via observational hints in the repo and home directory.
pub fn detect_agent_targets(repo_root: &Path, home: Option<&Path>) -> Vec<AgentTargetKind> {
    // Deterministic detection order: Claude, Cursor, Codex, Copilot, Windsurf.
    // Matches `all_agent_targets()` so callers can rely on first-hit semantics.
    let mut hits: Vec<AgentTargetKind> = Vec::new();
    for target in all_agent_targets() {
        if target_hint_present(repo_root, home, *target) {
            hits.push(*target);
        }
    }
    hits
}

fn target_hint_present(repo_root: &Path, home: Option<&Path>, target: AgentTargetKind) -> bool {
    let repo_hints: Vec<PathBuf> = match target {
        AgentTargetKind::Claude => vec![repo_root.join(".claude"), repo_root.join("CLAUDE.md")],
        AgentTargetKind::Cursor => vec![repo_root.join(".cursor")],
        AgentTargetKind::Codex => vec![
            repo_root.join(".codex"),
            repo_root.join(".agents").join("skills"),
        ],
        AgentTargetKind::Copilot => vec![repo_root.join(".github").join("copilot-instructions.md")],
        AgentTargetKind::Windsurf => vec![repo_root.join(".windsurf")],
        AgentTargetKind::Opencode => {
            vec![repo_root.join("opencode.json"), repo_root.join("AGENTS.md")]
        }
        other => vec![repo_root.join(format!(".{}", other.as_str()))],
    };
    if repo_hints.iter().any(|p| p.exists()) {
        return true;
    }
    let Some(home) = home else {
        return false;
    };
    let home_hints: Vec<PathBuf> = match target {
        AgentTargetKind::Claude => vec![home.join(".claude")],
        AgentTargetKind::Cursor => vec![home.join(".cursor")],
        AgentTargetKind::Codex => vec![home.join(".codex"), home.join(".agents").join("skills")],
        AgentTargetKind::Copilot => vec![],
        AgentTargetKind::Windsurf => vec![home.join(".windsurf")],
        AgentTargetKind::Opencode => vec![home.join(".opencode")],
        other => vec![home.join(format!(".{}", other.as_str()))],
    };
    home_hints.iter().any(|p| p.exists())
}

/// Resolve the user's home directory using platform environment variables.
pub fn dirs_home() -> Option<PathBuf> {
    // Minimal stdlib-only resolver to avoid pulling in `dirs`.
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
