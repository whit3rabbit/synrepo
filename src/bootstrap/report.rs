//! Bootstrap health states and report type.

use crate::config::Mode;
use std::path::{Path, PathBuf};

/// Shim paths (relative to repo root) written by `synrepo agent-setup <tool>`.
/// Listed in the preference order used when picking a pointer target.
const KNOWN_SHIM_PATHS: &[&str] = &[
    ".claude/synrepo-context.md",
    ".cursor/synrepo.mdc",
    ".codex/instructions.md",
    ".windsurf/rules/synrepo.md",
    "synrepo-copilot-instructions.md",
    "synrepo-agents.md",
];

fn first_existing_shim(repo_root: &Path) -> Option<PathBuf> {
    KNOWN_SHIM_PATHS
        .iter()
        .map(|rel| repo_root.join(rel))
        .find(|p| p.is_file())
}

/// Whether bootstrap completed cleanly or required repair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapHealth {
    /// All stores and config were consistent; no repair needed.
    Healthy,
    /// Runtime state was inconsistent and was automatically repaired.
    Degraded,
}

impl BootstrapHealth {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            BootstrapHealth::Healthy => "healthy",
            BootstrapHealth::Degraded => "degraded",
        }
    }
}

/// The action taken during a bootstrap run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapAction {
    /// `.synrepo/` did not exist and was created from scratch.
    Created,
    /// `.synrepo/` already existed and was refreshed in place.
    Refreshed,
    /// `.synrepo/` was partially initialised and was repaired.
    Repaired,
}

/// Human-readable summary of a bootstrap run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapReport {
    /// Whether the bootstrap completed cleanly or required repair.
    pub health: BootstrapHealth,
    /// The operational mode that was written to config.
    pub mode: Mode,
    /// Optional explanation of how the mode was selected or why it differs from the recommendation.
    pub mode_guidance: Option<String>,
    /// Zero or more compatibility advisory lines (warnings, info).
    pub compatibility_guidance: Vec<String>,
    /// Absolute path to the `.synrepo/` directory.
    pub synrepo_dir: PathBuf,
    /// Human-readable description of what the substrate index did.
    pub substrate_status: String,
    /// Human-readable description of what the structural compile produced.
    pub graph_status: String,
    /// Suggested next action for the user.
    pub next_step: String,
}

impl BootstrapReport {
    /// Render a human-readable summary suitable for CLI stdout.
    pub fn render(&self) -> String {
        let mut rendered = format!(
            "Bootstrap health: {}\nMode: {:?}\n",
            self.health.as_str(),
            self.mode,
        );
        if let Some(guidance) = &self.mode_guidance {
            rendered.push_str(&format!("Mode guidance: {}\n", guidance));
        }
        for guidance in &self.compatibility_guidance {
            rendered.push_str(&format!("Compatibility: {}\n", guidance));
        }
        rendered.push_str(&format!(
            "Runtime path: {}\nSubstrate: {}\nGraph: {}\nNext: {}\n",
            self.synrepo_dir.display(),
            self.substrate_status,
            self.graph_status,
            self.next_step
        ));
        if matches!(self.health, BootstrapHealth::Healthy) {
            rendered.push_str(&self.doctrine_pointer_line());
        }
        rendered
    }

    /// Single-line pointer to the agent doctrine. Names the first shim written
    /// by `synrepo agent-setup` if one exists under the repo root; otherwise
    /// directs the user to run `synrepo agent-setup <tool>`.
    fn doctrine_pointer_line(&self) -> String {
        let repo_root = self.synrepo_dir.parent();
        let target = repo_root
            .and_then(first_existing_shim)
            .map(|path| {
                let display = repo_root
                    .and_then(|root| path.strip_prefix(root).ok())
                    .map(|rel| rel.display().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                format!("See {display} for the full protocol.")
            })
            .unwrap_or_else(|| {
                "Run `synrepo agent-setup <tool>` to write a shim with the full protocol."
                    .to_string()
            });
        format!("Agent doctrine: tiny → normal → deep. {target}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn base_report(health: BootstrapHealth, synrepo_dir: PathBuf) -> BootstrapReport {
        BootstrapReport {
            health,
            mode: Mode::Auto,
            mode_guidance: None,
            compatibility_guidance: vec![],
            synrepo_dir,
            substrate_status: "built initial index".to_string(),
            graph_status: "compiled".to_string(),
            next_step: "run `synrepo search <query>`".to_string(),
        }
    }

    #[test]
    fn healthy_render_without_shim_points_at_agent_setup() {
        let repo = tempdir().unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let report = base_report(BootstrapHealth::Healthy, synrepo_dir);

        let rendered = report.render();
        assert!(rendered.contains("Agent doctrine: tiny → normal → deep."));
        assert!(rendered.contains("Run `synrepo agent-setup <tool>`"));
    }

    #[test]
    fn healthy_render_with_existing_shim_points_at_shim_path() {
        let repo = tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join(".claude")).unwrap();
        std::fs::write(
            repo.path().join(".claude").join("synrepo-context.md"),
            "# existing shim\n",
        )
        .unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let report = base_report(BootstrapHealth::Healthy, synrepo_dir);

        let rendered = report.render();
        assert!(rendered.contains("Agent doctrine: tiny → normal → deep."));
        assert!(rendered.contains(".claude/synrepo-context.md"));
        assert!(!rendered.contains("Run `synrepo agent-setup"));
    }

    #[test]
    fn degraded_render_omits_doctrine_pointer() {
        let repo = tempdir().unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let report = base_report(BootstrapHealth::Degraded, synrepo_dir);

        let rendered = report.render();
        assert!(!rendered.contains("Agent doctrine:"));
    }
}
