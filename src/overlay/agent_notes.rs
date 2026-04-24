//! Agent-note types for advisory overlay observations.

mod lifecycle;

pub use lifecycle::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use time::OffsetDateTime;

/// Stable source label returned with every note.
pub const AGENT_NOTE_SOURCE_STORE: &str = "overlay";

/// Target kind for an agent note.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentNoteTargetKind {
    /// Repo-relative source path.
    Path,
    /// Canonical file node ID.
    File,
    /// Canonical symbol node ID.
    Symbol,
    /// Canonical concept node ID.
    Concept,
    /// Test identifier or test path.
    Test,
    /// Card target string.
    Card,
    /// Existing note ID.
    Note,
}

impl AgentNoteTargetKind {
    /// Stable snake_case label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Concept => "concept",
            Self::Test => "test",
            Self::Card => "card",
            Self::Note => "note",
        }
    }
}

impl FromStr for AgentNoteTargetKind {
    type Err = crate::Error;

    fn from_str(value: &str) -> crate::Result<Self> {
        match value {
            "path" => Ok(Self::Path),
            "file" => Ok(Self::File),
            "symbol" => Ok(Self::Symbol),
            "concept" => Ok(Self::Concept),
            "test" => Ok(Self::Test),
            "card" => Ok(Self::Card),
            "note" => Ok(Self::Note),
            other => Err(crate::Error::Other(anyhow::anyhow!(
                "unsupported note target kind `{other}`"
            ))),
        }
    }
}

/// Explicit note target. Free-floating notes are rejected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentNoteTarget {
    /// Target kind.
    pub kind: AgentNoteTargetKind,
    /// Stable target ID or repo-relative path.
    pub id: String,
}

/// Confidence label supplied by the author.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentNoteConfidence {
    /// Low confidence advisory note.
    Low,
    /// Medium confidence advisory note.
    Medium,
    /// High confidence advisory note.
    High,
}

impl FromStr for AgentNoteConfidence {
    type Err = crate::Error;

    fn from_str(value: &str) -> crate::Result<Self> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(crate::Error::Other(anyhow::anyhow!(
                "unsupported note confidence `{other}`"
            ))),
        }
    }
}

impl AgentNoteConfidence {
    /// Stable snake_case label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Lifecycle status for an agent note.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentNoteStatus {
    /// Valid provenance and no known drift.
    Active,
    /// Valid shape but lacking verified evidence.
    Unverified,
    /// Cited source hashes, graph revision, or evidence references drifted.
    Stale,
    /// Replaced by a newer note.
    Superseded,
    /// Hidden from normal retrieval but retained for audit.
    Forgotten,
    /// Malformed or missing required provenance.
    Invalid,
}

impl AgentNoteStatus {
    /// Stable snake_case label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Unverified => "unverified",
            Self::Stale => "stale",
            Self::Superseded => "superseded",
            Self::Forgotten => "forgotten",
            Self::Invalid => "invalid",
        }
    }
}

impl FromStr for AgentNoteStatus {
    type Err = crate::Error;

    fn from_str(value: &str) -> crate::Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "unverified" => Ok(Self::Unverified),
            "stale" => Ok(Self::Stale),
            "superseded" => Ok(Self::Superseded),
            "forgotten" => Ok(Self::Forgotten),
            "invalid" => Ok(Self::Invalid),
            other => Err(crate::Error::Other(anyhow::anyhow!(
                "unsupported note status `{other}`"
            ))),
        }
    }
}

impl fmt::Display for AgentNoteStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Evidence reference cited by a note.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentNoteEvidence {
    /// Evidence kind, for example `symbol`, `file`, `test`, or `span`.
    pub kind: String,
    /// Evidence identifier.
    pub id: String,
}

/// Source-hash drift anchor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentNoteSourceHash {
    /// Repo-relative path for the anchored source file.
    pub path: String,
    /// Content hash observed when the note was written or verified.
    pub hash: String,
}

/// Advisory agent-authored note.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentNote {
    /// Stable note ID.
    pub note_id: String,
    /// Explicit repo target.
    pub target: AgentNoteTarget,
    /// Advisory claim.
    pub claim: String,
    /// Evidence references.
    #[serde(default)]
    pub evidence: Vec<AgentNoteEvidence>,
    /// Author/tool identity.
    pub created_by: String,
    /// Creation timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Last lifecycle update timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// Author confidence.
    pub confidence: AgentNoteConfidence,
    /// Current lifecycle status.
    pub status: AgentNoteStatus,
    /// Source hash anchors for drift detection.
    #[serde(default)]
    pub source_hashes: Vec<AgentNoteSourceHash>,
    /// Optional graph revision anchor.
    #[serde(default)]
    pub graph_revision: Option<u64>,
    /// Whether drift should stale the note.
    pub expires_on_drift: bool,
    /// Notes this note replaces.
    #[serde(default)]
    pub supersedes: Vec<String>,
    /// Replacing note ID.
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Verification timestamp.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub verified_at: Option<OffsetDateTime>,
    /// Verifier identity.
    #[serde(default)]
    pub verified_by: Option<String>,
    /// Drift or transition actor that invalidated this note.
    #[serde(default)]
    pub invalidated_by: Option<String>,
    /// Stable source label, always `overlay`.
    pub source_store: String,
    /// Advisory label, always true.
    pub advisory: bool,
}

impl AgentNote {
    /// Build a note with defaults for an explicit target and claim.
    pub fn new(
        target: AgentNoteTarget,
        claim: String,
        created_by: String,
        confidence: AgentNoteConfidence,
    ) -> Self {
        let now = OffsetDateTime::now_utc();
        let seed = format!(
            "{}\n{}\n{}\n{}\n{}",
            target.kind.as_str(),
            target.id,
            claim,
            created_by,
            now.unix_timestamp_nanos()
        );
        let digest = blake3::hash(seed.as_bytes());
        let note_id = format!("note_{}", &hex::encode(digest.as_bytes())[..24]);
        Self {
            note_id,
            target,
            claim,
            evidence: Vec::new(),
            created_by,
            created_at: now,
            updated_at: now,
            confidence,
            status: AgentNoteStatus::Active,
            source_hashes: Vec::new(),
            graph_revision: None,
            expires_on_drift: true,
            supersedes: Vec::new(),
            superseded_by: None,
            verified_at: None,
            verified_by: None,
            invalidated_by: None,
            source_store: AGENT_NOTE_SOURCE_STORE.to_string(),
            advisory: true,
        }
    }

    /// Classify the note before persistence.
    pub fn normalize_for_insert(&mut self) -> crate::Result<()> {
        self.source_store = AGENT_NOTE_SOURCE_STORE.to_string();
        self.advisory = true;
        self.target.id = self.target.id.trim().to_string();
        self.claim = self.claim.trim().to_string();
        self.created_by = self.created_by.trim().to_string();
        if self.note_id.trim().is_empty() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "agent note id must not be empty"
            )));
        }
        if self.target.id.is_empty() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "agent note target must not be empty"
            )));
        }
        if self.claim.is_empty() || self.created_by.is_empty() {
            self.status = AgentNoteStatus::Invalid;
        } else if self.evidence.is_empty() && self.status == AgentNoteStatus::Active {
            self.status = AgentNoteStatus::Unverified;
        }
        Ok(())
    }
}
