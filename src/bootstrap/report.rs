//! Bootstrap health states and report type.

use crate::config::Mode;
use std::path::PathBuf;

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
        rendered
    }
}
