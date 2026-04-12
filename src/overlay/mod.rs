//! The overlay store: LLM-authored content, physically separate from the graph.
//!
//! Defines the [`OverlayStore`] trait and its payload types: commentary
//! entries (with provenance, freshness derivation, and staleness repair), plus
//! cross-link proposals (stubbed; activated in phase 5+).
//!
//! The key invariant: the synthesis pipeline never reads its own previous
//! output as retrieval input. This is enforced both physically (overlay
//! lives in a separate sqlite database from the graph) and at the retrieval
//! layer (synthesis queries filter on `source_store = "graph"`).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::core::provenance::Provenance;

/// Epistemic origin of an overlay entry. These variants cannot appear in
/// the canonical graph — the type system enforces the boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayEpistemic {
    /// LLM-proposed with strong cited evidence and short graph distance.
    MachineAuthoredHighConf,
    /// LLM-proposed with weak evidence.
    MachineAuthoredLowConf,
}

/// Kinds of edges the synthesis layer can propose. These land in the
/// overlay's `cross_links` table, never in the canonical graph's `edges` table.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayEdgeKind {
    /// A prose document references a code symbol.
    References,
    /// A prose document describes governance of a code artifact,
    /// proposed (not human-declared — the `Governs` edge in the graph
    /// is the human-declared variant).
    Governs,
    /// One concept is derived from another.
    DerivedFrom,
    /// A prose document mentions a code identifier.
    Mentions,
}

/// A cited span — verbatim text the LLM extracted from a source artifact,
/// verified by fuzzy LCS match against the actual source after normalization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CitedSpan {
    /// The file or concept the span was cited from.
    pub artifact: NodeId,
    /// Normalized form of the span text (whitespace collapsed, Unicode
    /// quotes normalized, etc.). This is what the verifier compared against.
    pub normalized_text: String,
    /// Byte offset in the source artifact where the verifier found a match
    /// after fuzzy snapping. This is the authoritative location, not the
    /// offset the LLM claimed.
    pub verified_at_offset: u32,
    /// LCS ratio from the fuzzy match (in `[0.0, 1.0]`; >= 0.9 for default
    /// threshold, tunable per provider).
    pub lcs_ratio: f32,
}

/// A proposed cross-link in the overlay.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayLink {
    /// Source node.
    pub from: NodeId,
    /// Target node.
    pub to: NodeId,
    /// Relationship kind.
    pub kind: OverlayEdgeKind,
    /// Epistemic origin.
    pub epistemic: OverlayEpistemic,
    /// Cited spans from the source artifact.
    pub source_spans: Vec<CitedSpan>,
    /// Cited spans from the target artifact.
    pub target_spans: Vec<CitedSpan>,
    /// Confidence score derived from span count, span length, LCS ratios,
    /// and graph distance.
    pub confidence: f32,
    /// One-sentence LLM rationale, stored for audit but not used by the verifier.
    pub rationale: Option<String>,
    /// Provenance metadata including the prompt version and model ID.
    pub provenance: Provenance,
}

/// Provenance record for a single commentary entry.
///
/// Every commentary entry carries one of these; all fields are required and
/// validated on insert. A missing or empty field yields `FreshnessState::Invalid`
/// when derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentaryProvenance {
    /// Content hash (typically the annotated node's file's `content_hash`) at
    /// the time this commentary was generated. Used for freshness derivation.
    pub source_content_hash: String,
    /// Identifier of the generation pass that produced this entry (e.g. a
    /// human-readable pass name or a deterministic pass ID).
    pub pass_id: String,
    /// Identity of the model that produced this entry (e.g. `claude-sonnet-4-6`).
    pub model_identity: String,
    /// Generation timestamp (RFC 3339 UTC when serialized).
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
}

/// A single commentary entry persisted in the overlay store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommentaryEntry {
    /// The node this commentary annotates.
    pub node_id: NodeId,
    /// The commentary body text.
    pub text: String,
    /// Provenance record for this entry.
    pub provenance: CommentaryProvenance,
}

/// Observable freshness state of a commentary entry.
///
/// Mirrors the five spec states: a match against the current source yields
/// `Fresh`; a mismatch yields `Stale`; a present entry with missing
/// provenance yields `Invalid`; absence of any entry yields `Missing`; a
/// node kind with no commentary pipeline yields `Unsupported`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    /// Stored `source_content_hash` matches the current `FileNode.content_hash`.
    Fresh,
    /// Stored `source_content_hash` does not match the current source.
    Stale,
    /// Entry is present but missing one or more required provenance fields.
    Invalid,
    /// No entry exists for the queried node.
    Missing,
    /// The node kind has no commentary pipeline defined.
    Unsupported,
}

impl FreshnessState {
    /// Stable snake_case identifier for serialization and logging.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Invalid => "invalid",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Trait for the overlay store. Phase 1 ships commentary persistence via
/// `SqliteOverlayStore`; cross-link proposals remain stubbed until phase 5.
pub trait OverlayStore: Send + Sync {
    /// Insert a proposed cross-link. Phase 5+; stub implementations may
    /// return an error indicating the link surface is not yet active.
    fn insert_link(&mut self, link: OverlayLink) -> crate::Result<()>;

    /// Return all overlay links involving a given node (as source or target).
    /// Phase 5+; stub implementations may return an empty list.
    fn links_for(&self, node: NodeId) -> crate::Result<Vec<OverlayLink>>;

    /// Commit any pending writes (no-op for auto-commit SQLite connections).
    fn commit(&mut self) -> crate::Result<()>;

    /// Upsert a commentary entry, keyed on `node_id`. Rejects entries whose
    /// provenance is missing one or more required fields.
    fn insert_commentary(&mut self, entry: CommentaryEntry) -> crate::Result<()>;

    /// Return the commentary entry for a node, or `None` if absent.
    fn commentary_for(&self, node: NodeId) -> crate::Result<Option<CommentaryEntry>>;

    /// Delete all commentary entries whose `node_id` is not in `live_nodes`.
    /// Returns the number of rows deleted.
    fn prune_orphans(&mut self, live_nodes: &[NodeId]) -> crate::Result<usize>;
}
