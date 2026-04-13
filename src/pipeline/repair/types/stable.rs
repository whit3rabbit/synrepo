use serde::{Deserialize, Serialize};

/// A named repair surface: a logical unit that can be independently checked
/// and selectively repaired.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairSurface {
    /// Storage compatibility and cleanup state (graph, index stores).
    StoreMaintenance,
    /// Currency of the graph against current source files.
    StructuralRefresh,
    /// Writer lock ownership (can block repairs if held by another process).
    WriterLock,
    /// Governs edges referencing nodes that still exist.
    DeclaredLinks,
    /// Concept documents whose governed code targets have drifted.
    StaleRationale,
    /// Commentary overlay entries (LLM-authored symbol commentary in `.synrepo/overlay/overlay.db`).
    CommentaryOverlayEntries,
    /// Proposed cross-link overlay entries (LLM-proposed prose↔code links in
    /// `.synrepo/overlay/overlay.db`).
    ProposedLinksOverlay,
    /// Export directory freshness tracked via the export manifest.
    ExportSurface,
}

impl RepairSurface {
    /// Stable snake_case identifier for serialization and logging.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StoreMaintenance => "store_maintenance",
            Self::StructuralRefresh => "structural_refresh",
            Self::WriterLock => "writer_lock",
            Self::DeclaredLinks => "declared_links",
            Self::StaleRationale => "stale_rationale",
            Self::CommentaryOverlayEntries => "commentary_overlay_entries",
            Self::ProposedLinksOverlay => "proposed_links_overlay",
            Self::ExportSurface => "export_surface",
        }
    }
}

/// How a surface has drifted from its expected state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftClass {
    /// Surface is current; no drift detected.
    Current,
    /// Surface is stale and can be auto-repaired deterministically.
    Stale,
    /// Surface has never been materialized.
    Absent,
    /// Surface has a conflict requiring human judgment to resolve.
    TrustConflict,
    /// Surface is not yet implemented in this runtime.
    Unsupported,
    /// Surface repair is blocked by a prerequisite condition.
    Blocked,
    /// A cross-link candidate points at an endpoint that no longer exists.
    /// Distinct from `Stale` (which the deterministic revalidator can fix)
    /// because source-deleted candidates require manual review or pruning.
    SourceDeleted,
}

impl DriftClass {
    /// Stable snake_case identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Absent => "absent",
            Self::TrustConflict => "trust_conflict",
            Self::Unsupported => "unsupported",
            Self::Blocked => "blocked",
            Self::SourceDeleted => "source_deleted",
        }
    }
}

/// How severe a finding is and what kind of response it warrants.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Auto-repair is available and safe.
    Actionable,
    /// Finding is noted but repair requires human review.
    ReportOnly,
    /// Repair is blocked by an external condition.
    Blocked,
    /// Surface is not implemented; finding is informational only.
    Unsupported,
}

impl Severity {
    /// Stable snake_case identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Actionable => "actionable",
            Self::ReportOnly => "report_only",
            Self::Blocked => "blocked",
            Self::Unsupported => "unsupported",
        }
    }
}

/// What action the repair loop should take for a finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    /// No action needed; surface is current.
    None,
    /// Run a structural reconcile pass.
    RunReconcile,
    /// Clear/rebuild stale stores via maintenance plan.
    RunMaintenance,
    /// Run maintenance then reconcile to fully restore the surface.
    RunMaintenanceThenReconcile,
    /// Requires operator or human review; cannot auto-repair.
    ManualReview,
    /// Surface is not implemented; no action available.
    NotSupported,
    /// Re-run the commentary generator for stale commentary overlay entries.
    RefreshCommentary,
    /// Re-run the deterministic fuzzy-LCS verifier against stale cross-link
    /// candidates; refresh endpoint hashes on success, demote tier on failure.
    /// Never invokes the LLM; full regeneration uses a separate path.
    RevalidateLinks,
    /// Re-run `write_exports` to refresh the stale export directory.
    RegenerateExports,
}

impl RepairAction {
    /// Stable snake_case identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::RunReconcile => "run_reconcile",
            Self::RunMaintenance => "run_maintenance",
            Self::RunMaintenanceThenReconcile => "run_maintenance_then_reconcile",
            Self::ManualReview => "manual_review",
            Self::NotSupported => "not_supported",
            Self::RefreshCommentary => "refresh_commentary",
            Self::RevalidateLinks => "revalidate_links",
            Self::RegenerateExports => "regenerate_exports",
        }
    }
}
