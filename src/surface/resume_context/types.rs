#![allow(missing_docs)]

use serde::Serialize;

use crate::{overlay::AgentNoteCounts, pipeline::recent_activity::ActivityEntry};

pub const DEFAULT_RESUME_CONTEXT_LIMIT: usize = 10;
pub const MAX_RESUME_CONTEXT_LIMIT: usize = 50;
pub const DEFAULT_RESUME_CONTEXT_SINCE_DAYS: u32 = 14;
pub const MAX_RESUME_CONTEXT_SINCE_DAYS: u32 = 365;
pub const DEFAULT_RESUME_CONTEXT_TOKEN_CAP: usize = 2_000;

#[derive(Clone, Debug)]
pub struct ResumeContextRequest {
    pub limit: usize,
    pub since_days: u32,
    pub budget_tokens: usize,
    pub include_notes: bool,
}

impl Default for ResumeContextRequest {
    fn default() -> Self {
        Self {
            limit: DEFAULT_RESUME_CONTEXT_LIMIT,
            since_days: DEFAULT_RESUME_CONTEXT_SINCE_DAYS,
            budget_tokens: DEFAULT_RESUME_CONTEXT_TOKEN_CAP,
            include_notes: true,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ResumeContextPacket {
    pub schema_version: u32,
    pub packet_type: String,
    pub repo_root: String,
    pub generated_at: String,
    pub context_state: ResumeContextState,
    pub sections: ResumeContextSections,
    pub detail_pointers: Vec<DetailPointer>,
    pub omitted: Vec<OmittedItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResumeContextState {
    pub token_estimate: usize,
    pub token_cap: usize,
    pub truncation_applied: bool,
    pub omitted_count: usize,
    pub source_stores: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_hint: Option<MetricsHint>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MetricsHint {
    pub cards_served_total: u64,
    pub estimated_tokens_saved_total: u64,
    pub compact_outputs_total: u64,
    pub resume_context_responses_total: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResumeContextSections {
    pub changed_files: ChangedFilesSection,
    pub next_actions: NextActionsSection,
    pub recent_activity: RecentActivitySection,
    pub saved_notes: SavedNotesSection,
    pub validation: ValidationSection,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChangedFilesSection {
    pub source_store: String,
    pub count: usize,
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NextActionsSection {
    pub source_store: String,
    pub count: usize,
    pub items: Vec<crate::surface::handoffs::HandoffItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecentActivitySection {
    pub source_store: String,
    pub count: usize,
    pub activity: Vec<ActivityEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SavedNotesSection {
    pub source_store: String,
    pub advisory: bool,
    pub overlay_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<AgentNoteCounts>,
    pub count: usize,
    pub notes: Vec<AgentNoteSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentNoteSummary {
    pub note_id: String,
    pub target_kind: String,
    pub target: String,
    pub status: String,
    pub confidence: String,
    pub updated_at: String,
    pub claim_preview: String,
    pub source_store: String,
    pub advisory: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValidationSection {
    pub recommended_commands: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetailPointer {
    pub label: String,
    pub mcp: String,
    pub cli: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct OmittedItem {
    pub section: String,
    pub reason: String,
    pub omitted_count: usize,
}
