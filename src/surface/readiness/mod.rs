//! Capability readiness matrix shared by runtime probe, status, doctor, bootstrap,
//! and the dashboard.
//!
//! Why this lives here: degradation is a cross-cutting concern. Parser, git,
//! embedding, watch, reconcile, overlay, and compatibility subsystems each own
//! their native diagnostics. The matrix is a read-only projection over those
//! diagnostics that normalizes labels, severities, and next actions so every
//! renderer shows the same degradation story.

mod project_layout;
mod rows;

use crate::tui::probe::Severity;
use crate::{
    bootstrap::runtime_probe::ProbeReport, config::Config, surface::status_snapshot::StatusSnapshot,
};
use project_layout::project_layout_row;
use rows::{
    compatibility_row, embeddings_row, git_row, index_freshness_row, overlay_row, parser_row,
    watch_row,
};

#[cfg(test)]
mod tests;

/// Capability identifier. Stable string serialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Capability {
    /// Structural parser coverage (tree-sitter).
    Parser,
    /// Git-derived history/ownership/co-change intelligence.
    GitIntelligence,
    /// Manifest-backed source/test layout detection.
    ProjectLayout,
    /// Embedding index / semantic triage.
    Embeddings,
    /// File watcher / reconcile daemon.
    Watch,
    /// Freshness of the last reconcile pass.
    IndexFreshness,
    /// Overlay store availability (commentary, cross-links, agent notes).
    Overlay,
    /// Storage compatibility (schema / version).
    Compatibility,
}

impl Capability {
    /// Stable lowercase identifier for JSON output and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Parser => "parser",
            Capability::GitIntelligence => "git-intelligence",
            Capability::ProjectLayout => "project-layout",
            Capability::Embeddings => "embeddings",
            Capability::Watch => "watch",
            Capability::IndexFreshness => "index-freshness",
            Capability::Overlay => "overlay",
            Capability::Compatibility => "compatibility",
        }
    }

    fn display_label(self) -> &'static str {
        match self {
            Capability::Parser => "parser",
            Capability::GitIntelligence => "git intelligence",
            Capability::ProjectLayout => "project layout",
            Capability::Embeddings => "embeddings",
            Capability::Watch => "watch",
            Capability::IndexFreshness => "index freshness",
            Capability::Overlay => "overlay",
            Capability::Compatibility => "compatibility",
        }
    }
}

/// Readiness state for one capability.
///
/// The six labels were chosen to keep disabled (user opt-out) distinct from
/// unavailable (subsystem not present). Both render without alarm, but only
/// Unavailable implies the system missed an expected input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadinessState {
    /// Feature is operational and fully enabled.
    Supported,
    /// Feature is working but with reduced fidelity. Results are usable but
    /// must be labeled degraded by downstream renderers.
    Degraded,
    /// Subsystem was expected but is not present (e.g., no `.git/` dir).
    Unavailable,
    /// User opted out (e.g., `enable_semantic_triage = false`, watch not
    /// started). Not a problem; renderers should not alarm.
    Disabled,
    /// Present but out of date; refresh recommended.
    Stale,
    /// Recovery action required before the feature works.
    Blocked,
}

impl ReadinessState {
    /// Stable lowercase identifier for JSON output and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            ReadinessState::Supported => "supported",
            ReadinessState::Degraded => "degraded",
            ReadinessState::Unavailable => "unavailable",
            ReadinessState::Disabled => "disabled",
            ReadinessState::Stale => "stale",
            ReadinessState::Blocked => "blocked",
        }
    }

    /// Severity tag consumed by color / ordering logic in the dashboard.
    pub fn severity(self) -> Severity {
        match self {
            ReadinessState::Supported | ReadinessState::Disabled => Severity::Healthy,
            ReadinessState::Degraded | ReadinessState::Unavailable | ReadinessState::Stale => {
                Severity::Stale
            }
            ReadinessState::Blocked => Severity::Blocked,
        }
    }
}

/// One row of the capability readiness matrix.
#[derive(Clone, Debug)]
pub struct ReadinessRow {
    /// Which capability this row describes.
    pub capability: Capability,
    /// Current readiness state.
    pub state: ReadinessState,
    /// One-line detail suitable for display next to the state.
    pub detail: String,
    /// Recommended command or action. `None` when no action is needed.
    pub next_action: Option<String>,
}

impl ReadinessRow {
    /// Display label (e.g. "git intelligence"). Stable across renderers.
    pub fn label(&self) -> &'static str {
        self.capability.display_label()
    }
}

/// Ordered matrix of capability readiness rows.
///
/// The order is fixed (parser, git, project layout, embeddings, watch, index freshness,
/// overlay, compatibility) so renderers and tests can rely on it.
#[derive(Clone, Debug)]
pub struct ReadinessMatrix {
    /// Rows in stable display order.
    pub rows: Vec<ReadinessRow>,
}

impl ReadinessMatrix {
    /// Build the matrix from the existing probe, status snapshot, and config.
    ///
    /// The builder does not open any store: it is a projection over
    /// already-collected diagnostics.
    pub fn build(
        repo_root: &std::path::Path,
        probe: &ProbeReport,
        snapshot: &StatusSnapshot,
        config: &Config,
    ) -> Self {
        let rows = vec![
            parser_row(snapshot),
            git_row(repo_root, config),
            project_layout_row(repo_root, config),
            embeddings_row(snapshot, config),
            watch_row(snapshot),
            index_freshness_row(snapshot),
            overlay_row(snapshot),
            compatibility_row(probe, snapshot),
        ];
        ReadinessMatrix { rows }
    }

    /// Iterate over rows whose severity is not `Healthy`.
    pub fn degraded_rows(&self) -> impl Iterator<Item = &ReadinessRow> {
        self.rows
            .iter()
            .filter(|row| !matches!(row.state.severity(), Severity::Healthy))
    }
}
