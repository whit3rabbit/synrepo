use serde::{Deserialize, Serialize};

use crate::core::ids::FileNodeId;

use super::super::git::FileGitIntelligence;
use super::refs::FileRef;
use super::refs::SymbolRef;
use super::symbol::ProposedLink;
use super::{option_vec_is_empty, ContextAccounting, SourceStore};

/// FileCard — answers "what's in this file, what depends on it?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileCard {
    /// The file this card describes.
    pub file: FileNodeId,
    /// Path relative to the repo root.
    pub path: String,
    /// Top-level symbols in the file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<SymbolRef>,
    /// Files that import this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imported_by: Vec<FileRef>,
    /// Files this one imports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<FileRef>,
    /// Files that co-change with this one without an import edge (hidden coupling).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub co_changes: Vec<FileRef>,
    /// Git-derived recent change context for this file, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_intelligence: Option<FileGitIntelligence>,
    /// Drift flag summary across edges incident to this file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_flag: Option<String>,
    /// Approximate token count.
    pub approx_tokens: usize,
    /// Context-accounting metadata for this card.
    pub context_accounting: ContextAccounting,
    /// Source store.
    pub source_store: SourceStore,
    /// Proposed cross-links authored by the explain layer.
    #[serde(default, skip_serializing_if = "option_vec_is_empty")]
    pub proposed_links: Option<Vec<ProposedLink>>,
    /// State of the proposed links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links_state: Option<String>,
}
