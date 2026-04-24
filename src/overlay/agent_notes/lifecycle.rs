//! Agent-note lifecycle and query helper types.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{AgentNoteStatus, AgentNoteTargetKind};

/// Lifecycle transition action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentNoteTransitionAction {
    /// Note was created.
    Add,
    /// Note was linked to another note.
    Link,
    /// Note superseded another note.
    Supersede,
    /// Note was forgotten.
    Forget,
    /// Note was verified.
    Verify,
    /// Note was marked stale or invalid.
    Invalidate,
}

impl AgentNoteTransitionAction {
    /// Stable snake_case label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Link => "link",
            Self::Supersede => "supersede",
            Self::Forget => "forget",
            Self::Verify => "verify",
            Self::Invalidate => "invalidate",
        }
    }
}

/// Append-only lifecycle transition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentNoteTransition {
    /// Target note.
    pub note_id: String,
    /// Transition kind.
    pub action: AgentNoteTransitionAction,
    /// Previous status.
    pub previous_status: Option<AgentNoteStatus>,
    /// New status.
    pub new_status: AgentNoteStatus,
    /// Actor identity.
    pub actor: String,
    /// Optional reason.
    pub reason: Option<String>,
    /// Related note for link/supersede transitions.
    pub related_note: Option<String>,
    /// Transition timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub happened_at: OffsetDateTime,
}

/// Query options for note retrieval.
#[derive(Clone, Debug)]
pub struct AgentNoteQuery {
    /// Optional target kind filter.
    pub target_kind: Option<AgentNoteTargetKind>,
    /// Optional target ID filter.
    pub target_id: Option<String>,
    /// Include forgotten tombstones.
    pub include_forgotten: bool,
    /// Include superseded notes.
    pub include_superseded: bool,
    /// Include invalid notes.
    pub include_invalid: bool,
    /// Maximum rows returned.
    pub limit: usize,
}

impl Default for AgentNoteQuery {
    fn default() -> Self {
        Self {
            target_kind: None,
            target_id: None,
            include_forgotten: false,
            include_superseded: false,
            include_invalid: false,
            limit: 20,
        }
    }
}

/// Lifecycle counts surfaced in status and repair.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentNoteCounts {
    /// Active notes.
    pub active: usize,
    /// Unverified notes.
    pub unverified: usize,
    /// Stale notes.
    pub stale: usize,
    /// Superseded notes.
    pub superseded: usize,
    /// Forgotten notes.
    pub forgotten: usize,
    /// Invalid notes.
    pub invalid: usize,
}

impl AgentNoteCounts {
    /// Total notes counted.
    pub fn total(self) -> usize {
        self.active + self.unverified + self.stale + self.superseded + self.forgotten + self.invalid
    }
}
