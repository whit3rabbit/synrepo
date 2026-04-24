//! Commentary entry types for the overlay store.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::core::ids::NodeId;

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
