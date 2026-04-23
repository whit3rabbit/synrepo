//! Agent-integration detection functions.

use std::{
    fs,
    path::{Path, PathBuf},
};

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

    for target in probe_order {
        let shim = shim_exists(repo_root, target);
        let mcp = mcp_registration_present(repo_root, target);
        match (shim, mcp) {
            (true, true) => return AgentIntegration::Complete { target },
            (true, false) => return AgentIntegration::Partial { target },
            (false, _) => continue,
        }
    }
    AgentIntegration::Absent
}

/// All known agent target kinds in stable order.
pub fn all_agent_targets() -> &'static [AgentTargetKind] {
    &[
        AgentTargetKind::Claude,
        AgentTargetKind::Cursor,
        AgentTargetKind::Codex,
        AgentTargetKind::Copilot,
        AgentTargetKind::Windsurf,
    ]
}

fn shim_exists(repo_root: &Path, target: AgentTargetKind) -> bool {
    shim_output_path(repo_root, target).exists()
}

/// Path to the shim output file for the given target.
pub fn shim_output_path(repo_root: &Path, target: AgentTargetKind) -> PathBuf {
    // Mirrors `AgentTool::output_path` in `src/bin/cli_support/agent_shims/mod.rs`.
    // Duplicated here deliberately: the library crate cannot depend on the
    // binary-crate-private agent_shims module. The pair of paths is narrow
    // and change-resistant; the test suite for Phase 3 asserts per-target
    // behavior, which will flag divergence.
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
            .join(".codex")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        AgentTargetKind::Copilot => repo_root.join("synrepo-copilot-instructions.md"),
        AgentTargetKind::Windsurf => repo_root
            .join(".windsurf")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
    }
}

fn mcp_registration_present(repo_root: &Path, target: AgentTargetKind) -> bool {
    match target {
        AgentTargetKind::Claude => claude_mcp_registered(repo_root),
        AgentTargetKind::Codex => codex_mcp_registered(repo_root),
        // Cursor/Copilot/Windsurf do not have a canonical project-scoped MCP
        // registration file today. The shim is the full integration signal.
        AgentTargetKind::Cursor | AgentTargetKind::Copilot | AgentTargetKind::Windsurf => {
            shim_exists(repo_root, target)
        }
    }
}

fn claude_mcp_registered(repo_root: &Path) -> bool {
    let path = repo_root.join(".mcp.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    v.get("mcpServers").and_then(|s| s.get("synrepo")).is_some()
}

fn codex_mcp_registered(repo_root: &Path) -> bool {
    let path = repo_root.join(".codex").join("config.toml");
    let Ok(text) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    doc.get("mcp")
        .and_then(|i| i.as_table())
        .and_then(|t| t.get("synrepo"))
        .is_some()
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
        AgentTargetKind::Codex => vec![repo_root.join(".codex")],
        AgentTargetKind::Copilot => vec![repo_root.join(".github").join("copilot-instructions.md")],
        AgentTargetKind::Windsurf => vec![repo_root.join(".windsurf")],
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
        AgentTargetKind::Codex => vec![home.join(".codex")],
        AgentTargetKind::Copilot => vec![],
        AgentTargetKind::Windsurf => vec![home.join(".windsurf")],
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
