use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use crate::overlay::{ConfidenceTier, CrossLinkFreshness, OverlayEdgeKind};
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
    /// Flat commentary state label exposed to MCP callers so they can
    /// distinguish `budget_withheld` (Tiny/Normal) from `missing`, `fresh`,
    /// `stale`, `invalid`, or `unsupported` (Deep). Parallel to
    /// `overlay_commentary` so callers can branch on the state without
    /// deserializing the nested object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commentary_state: Option<String>,
    /// Proposed cross-links authored by the synthesis layer with evidence verification.
    /// Only populated at Deep budget.
    pub proposed_links: Option<Vec<ProposedLink>>,
    /// State of the proposed links (e.g., "budget_withheld", "fresh", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links_state: Option<String>,
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
///
/// Mirrors the five spec states from `FreshnessState` in `src/overlay/mod.rs`:
/// `Fresh`, `Stale`, `Invalid`, `Missing`, `Unsupported`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// The commentary is current with the source it describes.
    Fresh,
    /// The source has changed since the commentary was produced.
    Stale,
    /// Entry is present but missing one or more required provenance fields.
    Invalid,
    /// No commentary exists for this target yet.
    Missing,
    /// The node kind has no commentary pipeline defined.
    Unsupported,
}

impl From<crate::overlay::FreshnessState> for Freshness {
    fn from(state: crate::overlay::FreshnessState) -> Self {
        match state {
            crate::overlay::FreshnessState::Fresh => Self::Fresh,
            crate::overlay::FreshnessState::Stale => Self::Stale,
            crate::overlay::FreshnessState::Invalid => Self::Invalid,
            crate::overlay::FreshnessState::Missing => Self::Missing,
            crate::overlay::FreshnessState::Unsupported => Self::Unsupported,
        }
    }
}

/// A proposed cross-link surfaced on a structural card.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposedLink {
    /// Node ID of the source.
    pub source: NodeId,
    /// Node ID of the target.
    pub target: NodeId,
    /// Kind of edge proposed.
    pub kind: OverlayEdgeKind,
    /// Confidence tier.
    pub tier: ConfidenceTier,
    /// Freshness of this proposed link compared to the current file content.
    pub freshness: CrossLinkFreshness,
    /// Number of spans cited as evidence.
    pub span_count: usize,
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
    /// Proposed cross-links authored by the synthesis layer.
    pub proposed_links: Option<Vec<ProposedLink>>,
    /// State of the proposed links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links_state: Option<String>,
}

/// ModuleCard — answers "what lives in this directory/module?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleCard {
    /// Path of the directory or module root (e.g. `src/auth/`).
    pub path: String,
    /// Top-level files directly inside this directory (not in subdirectories).
    pub files: Vec<FileRef>,
    /// Immediate subdirectory paths; agents can request ModuleCards for each.
    pub nested_modules: Vec<String>,
    /// Top-level public symbols visible across module boundaries.
    /// Empty at `Tiny` budget; populated at `Normal` and `Deep`.
    pub public_symbols: Vec<SymbolRef>,
    /// Total count of public symbols across all direct files (always populated).
    pub total_symbol_count: usize,
    /// Approximate token count.
    pub approx_tokens: usize,
    /// Source store.
    pub source_store: SourceStore,
}

/// Classification of an execution entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointKind {
    /// Binary entry point: a `main` function in `src/main.rs` or `src/bin/`.
    Binary,
    /// CLI command handler in a file whose path contains `cli`, `command`, or `cmd`.
    CliCommand,
    /// HTTP route handler: name starts with `handle_`, `serve_`, or `route_`,
    /// or the file path contains `handler`, `route`, or `router`.
    HttpHandler,
    /// Public item at a library root (`src/lib.rs` or a `mod.rs` boundary).
    LibRoot,
}

/// A single detected execution entry point.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPoint {
    /// Stable node ID of the entry-point symbol.
    pub symbol: SymbolNodeId,
    /// Fully qualified name within its file.
    pub qualified_name: String,
    /// File path and byte offset (e.g. `src/main.rs:0`).
    pub location: String,
    /// Classification of this entry point.
    pub kind: EntryPointKind,
    /// Number of unique callers in the graph. `None` at `Tiny` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_count: Option<usize>,
    /// Doc comment truncated to 80 characters. `None` at `Tiny` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// One-line signature. `None` below `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// EntryPointCard — answers "where does execution start in this scope?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPointCard {
    /// Optional path-prefix scope that was requested (`None` = whole repo).
    pub scope: Option<String>,
    /// Detected entry points, sorted by kind then file path, capped at 20.
    pub entry_points: Vec<EntryPoint>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` for entry-point cards).
    pub source_store: SourceStore,
}
