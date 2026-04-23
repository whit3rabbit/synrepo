//! Type definitions for the runtime probe.

use std::path::PathBuf;

/// One concrete thing the probe found missing or blocked that keeps a repo out
/// of `Ready` state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Missing {
    /// `.synrepo/config.toml` does not exist.
    ConfigFile,
    /// `.synrepo/config.toml` exists but is not readable or parseable.
    ConfigUnreadable {
        /// Human-readable explanation (I/O error or TOML parse error).
        detail: String,
    },
    /// The graph SQLite file `.synrepo/graph/nodes.db` is missing.
    GraphStore,
    /// Storage-compatibility evaluation reports a blocking action
    /// (`migrate-required` or `block` on a canonical store).
    CompatBlocked {
        /// Guidance lines lifted from the compat report, one per affected
        /// store, suitable for direct display in the repair wizard.
        guidance: Vec<String>,
    },
    /// The compat evaluation itself failed (I/O error, malformed snapshot not
    /// recoverable by a warning, etc.). The probe cannot classify the store
    /// cleanly so it reports partial with this cause.
    CompatEvaluationFailed {
        /// Underlying error formatted via `Display`.
        detail: String,
    },
}

/// Runtime-readiness classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeClassification {
    /// `.synrepo/` does not exist.
    Uninitialized,
    /// `.synrepo/` exists but one or more required components are missing or
    /// blocked. `missing` is never empty when this variant is used.
    Partial {
        /// Components that must be repaired before the repo is `Ready`.
        missing: Vec<Missing>,
    },
    /// All required components present and compat-clean.
    Ready,
}

/// Agent-target identifier for observational detection. Kept narrow to the
/// targets the wizard offers in v1; additional targets can be added without
/// touching the classification contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentTargetKind {
    /// Claude Code (`.claude/` or `CLAUDE.md`).
    Claude,
    /// Cursor (`.cursor/`).
    Cursor,
    /// OpenAI Codex CLI (`.codex/`).
    Codex,
    /// GitHub Copilot (`.github/copilot-instructions.md` or `copilot-*`).
    Copilot,
    /// Windsurf (`.windsurf/`).
    Windsurf,
}

impl AgentTargetKind {
    /// Stable lowercase identifier used in serialized output.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentTargetKind::Claude => "claude",
            AgentTargetKind::Cursor => "cursor",
            AgentTargetKind::Codex => "codex",
            AgentTargetKind::Copilot => "copilot",
            AgentTargetKind::Windsurf => "windsurf",
        }
    }
}

/// Supplementary agent-integration signal reported alongside runtime readiness.
///
/// Orthogonal to `RuntimeClassification`: a repo may be `Ready` with any of
/// these values. Used by the dashboard header and the integration sub-wizard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentIntegration {
    /// No shim present for any known target.
    Absent,
    /// Shim present for `target` but MCP registration missing.
    Partial {
        /// The detected target that has a shim but no MCP registration.
        target: AgentTargetKind,
    },
    /// Shim present and MCP registered for `target`.
    Complete {
        /// The fully-configured target.
        target: AgentTargetKind,
    },
}

impl AgentIntegration {
    /// Configured target, if any. Returns `None` for `Absent`.
    pub fn target(&self) -> Option<AgentTargetKind> {
        match self {
            AgentIntegration::Absent => None,
            AgentIntegration::Partial { target } | AgentIntegration::Complete { target } => {
                Some(*target)
            }
        }
    }
}

/// Structured output of [`probe`](super::probe).
#[derive(Clone, Debug)]
pub struct ProbeReport {
    /// Runtime readiness classification.
    pub classification: RuntimeClassification,
    /// Agent-integration signal (always reported, even when runtime is not
    /// `Ready`; callers typically only surface it in the `Ready` case).
    pub agent_integration: AgentIntegration,
    /// Deterministic ordered list of agent-target candidates detected via
    /// observational hints in the repo and `$HOME`. Used by the setup wizard
    /// to pre-select a default; empty means no hints matched.
    pub detected_agent_targets: Vec<AgentTargetKind>,
    /// `.synrepo/` path that was probed (regardless of whether it exists).
    pub synrepo_dir: PathBuf,
}

/// Routing decision derived from a [`ProbeReport`]. Consumed by the bare-
/// `synrepo` entrypoint to select between setup wizard, repair wizard,
/// dashboard, or a fallback-help path.
///
/// Contract (tested): `Partial` MUST route to `OpenRepair`, never to
/// `OpenSetup`. Preserving existing `.synrepo/` state is non-negotiable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingDecision {
    /// Repo has no `.synrepo/`; open the guided setup wizard.
    OpenSetup,
    /// Repo has `.synrepo/` but is missing required components; open the
    /// repair wizard showing the structured `missing` list.
    OpenRepair {
        /// Components that must be repaired. Copied from the probe report.
        missing: Vec<Missing>,
    },
    /// Repo is ready; open the dashboard. `integration` rides along so the
    /// dashboard header can show agent-integration status on open.
    OpenDashboard {
        /// Supplementary agent-integration signal from the probe.
        integration: AgentIntegration,
    },
}

impl RoutingDecision {
    /// Derive a routing decision from a probe report.
    pub fn from_report(report: &ProbeReport) -> Self {
        match &report.classification {
            RuntimeClassification::Uninitialized => RoutingDecision::OpenSetup,
            RuntimeClassification::Partial { missing } => RoutingDecision::OpenRepair {
                missing: missing.clone(),
            },
            RuntimeClassification::Ready => RoutingDecision::OpenDashboard {
                integration: report.agent_integration.clone(),
            },
        }
    }
}
