//! Bootstrap health states and report type.

use crate::bootstrap::runtime_probe::{all_agent_targets, shim_output_path};
use crate::config::Mode;
use std::path::{Path, PathBuf};

fn first_existing_shim(repo_root: &Path) -> Option<PathBuf> {
    all_agent_targets()
        .iter()
        .map(|target| shim_output_path(repo_root, *target))
        .chain([repo_root.join("synrepo-agents.md")])
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

/// One degraded capability observed at the end of bootstrap. Surfaces the
/// readiness matrix state so the success output labels optional features that
/// are off and core features that need a follow-up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DegradedCapability {
    /// Stable capability identifier (e.g. `git-intelligence`, `embeddings`).
    pub capability: String,
    /// Stable readiness label (`degraded`, `unavailable`, `disabled`, `stale`, `blocked`).
    pub state: String,
    /// One-line detail suitable for display next to the capability name.
    pub detail: String,
    /// Recommended next action, if any.
    pub next_action: Option<String>,
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
    /// Capabilities that ended bootstrap in a non-healthy readiness state.
    /// Populated from the shared capability readiness matrix so bootstrap,
    /// status, doctor, and the dashboard all report the same degradation.
    pub degraded_capabilities: Vec<DegradedCapability>,
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
        if !self.degraded_capabilities.is_empty() {
            rendered.push_str("Degraded capabilities:\n");
            for cap in &self.degraded_capabilities {
                let action = match &cap.next_action {
                    Some(a) => format!(" — {a}"),
                    None => String::new(),
                };
                rendered.push_str(&format!(
                    "  {} [{}]: {}{}\n",
                    cap.capability, cap.state, cap.detail, action,
                ));
            }
        }
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
                "Run `synrepo agent-setup <tool>` to write the agent skill or instructions file with the full protocol."
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
            degraded_capabilities: vec![],
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
        let claude_skill_dir = repo.path().join(".claude").join("skills").join("synrepo");
        std::fs::create_dir_all(&claude_skill_dir).unwrap();
        std::fs::write(
            claude_skill_dir.join("SKILL.md"),
            "---\nname: synrepo\ndescription: test shim\n---\n\n# existing shim\n",
        )
        .unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let report = base_report(BootstrapHealth::Healthy, synrepo_dir);

        let rendered = report.render();
        assert!(rendered.contains("Agent doctrine: tiny → normal → deep."));
        // `Path::display()` uses the platform separator (`/` on Unix, `\` on
        // Windows), so build the expected substring the same way rather than
        // hardcoding forward slashes.
        let expected = std::path::PathBuf::from(".claude")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md");
        let expected_display = expected.display().to_string();
        assert!(
            rendered.contains(&expected_display),
            "expected rendered output to contain {expected_display:?}, got {rendered:?}"
        );
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

    #[test]
    fn render_lists_degraded_capabilities_with_next_actions() {
        // Scenario: bootstrap completes but a core capability or optional
        // feature ended in a non-healthy readiness state. The success output
        // must name the capability, label the state, and carry the next action
        // from the shared readiness matrix.
        let repo = tempdir().unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let mut report = base_report(BootstrapHealth::Healthy, synrepo_dir);
        report.degraded_capabilities = vec![
            DegradedCapability {
                capability: "git-intelligence".to_string(),
                state: "unavailable".to_string(),
                detail: "no git repository".to_string(),
                next_action: Some(
                    "initialize git with `git init` to enable history-derived facts".to_string(),
                ),
            },
            DegradedCapability {
                capability: "index-freshness".to_string(),
                state: "stale".to_string(),
                detail: "no reconcile recorded".to_string(),
                next_action: Some("run `synrepo reconcile`".to_string()),
            },
        ];

        let rendered = report.render();
        assert!(rendered.contains("Degraded capabilities:"));
        assert!(rendered.contains("git-intelligence [unavailable]"));
        assert!(rendered.contains("no git repository"));
        assert!(rendered.contains("git init"));
        assert!(rendered.contains("index-freshness [stale]"));
        assert!(rendered.contains("synrepo reconcile"));
    }

    #[test]
    fn render_omits_degraded_block_when_none() {
        let repo = tempdir().unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let report = base_report(BootstrapHealth::Healthy, synrepo_dir);

        let rendered = report.render();
        assert!(
            !rendered.contains("Degraded capabilities:"),
            "rendered output must not emit the degraded block when there are none"
        );
    }
}
