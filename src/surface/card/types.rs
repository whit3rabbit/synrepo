use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, SymbolNodeId};
use crate::structure::graph::Epistemic;

use super::{FileGitIntelligence, SourceStore};

/// A reference to a caller or callee in a SymbolCard.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolRef {
    /// Node ID of the referenced symbol.
    pub id: SymbolNodeId,
    /// Qualified name for display.
    pub qualified_name: String,
    /// File path and line for display.
    pub location: String,
}

/// A reference to a file in a FileCard or similar.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileRef {
    /// Node ID of the referenced file.
    pub id: FileNodeId,
    /// Path relative to the repo root.
    pub path: String,
}

/// SymbolCard — answers "what is this function/class, how is it connected?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolCard {
    /// The symbol this card describes.
    pub symbol: SymbolNodeId,
    /// Display name (short form).
    pub name: String,
    /// Fully qualified name within its file.
    pub qualified_name: String,
    /// File and line where defined.
    pub defined_at: String,
    /// One-line signature.
    pub signature: Option<String>,
    /// Doc comment, truncated for `tiny` budget.
    pub doc_comment: Option<String>,
    /// Callers (symbols that call this one). Truncated per budget.
    pub callers: Vec<SymbolRef>,
    /// Callees (symbols this one calls). Truncated per budget.
    pub callees: Vec<SymbolRef>,
    /// Test symbols that exercise this one. Empty for `tiny`.
    pub tests_touching: Vec<SymbolRef>,
    /// Human-readable description of the last meaningful change.
    pub last_change: Option<String>,
    /// Drift score and flag, if any.
    pub drift_flag: Option<String>,
    /// Full source body, only populated for `Deep` budget.
    pub source_body: Option<String>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Every field in this card came from the graph; synthesis commentary
    /// is a separate field below if present.
    pub source_store: SourceStore,
    /// Epistemic origin of the primary fields.
    pub epistemic: Epistemic,
    /// Optional LLM-authored commentary from the overlay, clearly marked.
    /// Only populated if the card was requested at `Deep` budget and
    /// commentary exists in the overlay.
    pub overlay_commentary: Option<OverlayCommentary>,
}

/// LLM-authored commentary layered on top of a structural card.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayCommentary {
    /// The commentary text.
    pub text: String,
    /// Freshness state of the commentary.
    pub freshness: Freshness,
    /// Source store is always `Overlay` for commentary.
    pub source_store: SourceStore,
}

/// Freshness state of an overlay entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// The commentary is current with the source it describes.
    Fresh,
    /// The source has changed since the commentary was produced.
    Stale,
    /// No commentary exists for this target yet.
    Missing,
}

/// FileCard — answers "what's in this file, what depends on it?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileCard {
    /// The file this card describes.
    pub file: FileNodeId,
    /// Path relative to the repo root.
    pub path: String,
    /// Top-level symbols in the file.
    pub symbols: Vec<SymbolRef>,
    /// Files that import this one.
    pub imported_by: Vec<FileRef>,
    /// Files this one imports.
    pub imports: Vec<FileRef>,
    /// Files that co-change with this one without an import edge (hidden coupling).
    pub co_changes: Vec<FileRef>,
    /// Git-derived recent change context for this file, if available.
    pub git_intelligence: Option<FileGitIntelligence>,
    /// Drift flag summary across edges incident to this file.
    pub drift_flag: Option<String>,
    /// Approximate token count.
    pub approx_tokens: usize,
    /// Source store.
    pub source_store: SourceStore,
}

/// ModuleCard — answers "what lives in this directory/module?"
///
/// Struct only for now; the compiler method is added in a future slice once
/// module-boundary detection is wired into the structural pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleCard {
    /// Path of the directory or module root (e.g. `src/auth/`).
    pub path: String,
    /// Top-level files directly inside this directory.
    pub files: Vec<FileRef>,
    /// Top-level public symbols visible across module boundaries.
    pub public_symbols: Vec<SymbolRef>,
    /// Approximate token count.
    pub approx_tokens: usize,
    /// Source store.
    pub source_store: SourceStore,
}
