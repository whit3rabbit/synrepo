#![allow(missing_docs)]

use serde::Serialize;

use crate::overlay::{
    AgentNoteConfidence, AgentNoteEvidence, AgentNoteSourceHash, AgentNoteTargetKind,
};

#[derive(Clone, Debug)]
pub struct LessonAdd {
    pub target_kind: AgentNoteTargetKind,
    pub target: String,
    pub claim: String,
    pub created_by: String,
    pub confidence: AgentNoteConfidence,
    pub evidence: Vec<AgentNoteEvidence>,
    pub source_hashes: Vec<AgentNoteSourceHash>,
    pub graph_revision: Option<u64>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct LessonQuery {
    pub query: Option<String>,
    pub target_kind: Option<AgentNoteTargetKind>,
    pub target: Option<String>,
    pub limit: usize,
    pub include_hidden: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LessonView {
    pub lesson_id: String,
    pub target_kind: String,
    pub target: String,
    pub claim: String,
    pub status: String,
    pub freshness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub confidence: String,
    pub updated_at: String,
    pub evidence: Vec<AgentNoteEvidence>,
    pub source_hashes: Vec<AgentNoteSourceHash>,
    pub source_store: String,
    pub advisory: bool,
}
