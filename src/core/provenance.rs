//! Provenance metadata attached to every graph row and overlay entry.
//!
//! Verbose on purpose. Auditability is the value proposition versus RAG —
//! every fact in synrepo can be traced to the exact source, revision, and
//! pipeline stage that produced it.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::core::ids::FileNodeId;

/// Which pipeline produced a given row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatedBy {
    /// The structural pipeline (tree-sitter parse, markdown parse, git mine, drift scoring).
    StructuralPipeline,
    /// The synthesis pipeline (LLM-driven, produces overlay content only).
    SynthesisPipeline,
    /// A human acting via the CLI (e.g. `synrepo concept promote`).
    Human,
    /// The bootstrap command, for first-run initialization.
    Bootstrap,
}

/// A reference to a source artifact used to produce a row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRef {
    /// File node ID, if the source is a file already in the graph.
    pub file_id: Option<FileNodeId>,
    /// Path relative to the repo root.
    pub path: String,
    /// Content hash of the source at the time the row was produced.
    pub content_hash: String,
}

/// Provenance metadata. Every graph row and every overlay entry carries one.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    /// When this row was first created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Git revision at the time of creation.
    pub source_revision: String,
    /// Which pipeline created this row.
    pub created_by: CreatedBy,
    /// Name of the specific pass within the pipeline (e.g. "parse_code",
    /// "git_mine_cochange", "propose_link"). Free-form but stable within
    /// each pipeline release.
    pub pass: String,
    /// Source artifacts this row was derived from.
    pub source_artifacts: Vec<SourceRef>,
}

impl Provenance {
    /// Build a provenance record for a structural-pipeline row.
    pub fn structural(pass: &str, revision: &str, sources: Vec<SourceRef>) -> Self {
        Self {
            created_at: OffsetDateTime::now_utc(),
            source_revision: revision.to_string(),
            created_by: CreatedBy::StructuralPipeline,
            pass: pass.to_string(),
            source_artifacts: sources,
        }
    }
}
