//! Public row-level types surfaced to repair and status callers.

/// A cross-link row stuck in `pending_promotion` state after a crash during
/// the `links_accept` three-phase commit. Repair logic resolves these by
/// checking whether the corresponding graph edge was written.
#[derive(Clone, Debug)]
pub struct PendingPromotionRow {
    /// Source endpoint node ID (display form).
    pub from_node: String,
    /// Target endpoint node ID (display form).
    pub to_node: String,
    /// `OverlayEdgeKind` snake_case identifier.
    pub kind: String,
    /// Reviewer who initiated the promotion, if recorded.
    pub reviewer: Option<String>,
}

/// Counts of cross-link rows by state. Used by status and repair surfaces.
#[derive(Clone, Debug, Default)]
pub struct CrossLinkStateCounts {
    /// Number of rows in `active` state (awaiting review).
    pub active: usize,
    /// Number of rows in `pending_promotion` state (crash-recovery window).
    pub pending_promotion: usize,
    /// Number of rows in `promoted` state (written to graph).
    pub promoted: usize,
    /// Number of rows in `rejected` state (rejected by reviewer).
    pub rejected: usize,
}

/// Row-level snapshot used by the repair loop. Strings are the stored
/// serialized forms — node IDs in display format and the tier/state enums'
/// snake_case identifiers.
#[derive(Clone, Debug)]
pub struct CrossLinkHashRow {
    /// Source endpoint node ID (display form).
    pub from_node: String,
    /// Target endpoint node ID (display form).
    pub to_node: String,
    /// `OverlayEdgeKind` snake_case identifier.
    pub kind: String,
    /// Source endpoint content hash as of generation time.
    pub from_content_hash: String,
    /// Target endpoint content hash as of generation time.
    pub to_content_hash: String,
    /// `ConfidenceTier` snake_case identifier.
    pub confidence_tier: String,
    /// `CrossLinkState` snake_case identifier.
    pub state: String,
}
