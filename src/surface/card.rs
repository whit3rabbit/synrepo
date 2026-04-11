//! Cards: the user-facing unit of compressed context.
//!
//! Cards are small, structured, deterministic records compiled from the
//! graph and source code. They are not prose summaries. See
//! `synrepo-design-v4.md` section "Cards and the context budget protocol"
//! for the full rationale.

use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use crate::structure::graph::Epistemic;

/// Context budget tier for a card request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Budget {
    /// Roughly 200 tokens per card, ~1k tokens total.
    /// Card headers only: name, signature, location, top 3 callers/callees,
    /// drift flag. The default for orientation and routing.
    #[default]
    Tiny,
    /// Roughly 500 tokens per card, ~3k tokens total.
    /// Full card including test surface and recent change context.
    Normal,
    /// Roughly 2k tokens per card, ~10k tokens total.
    /// Full card plus actual source body, plus linked DecisionCards if available.
    /// Only for when the agent is about to write code that depends on the exact source.
    Deep,
}

impl Budget {
    /// Approximate per-card token budget for this tier.
    pub fn per_card_tokens(self) -> usize {
        match self {
            Budget::Tiny => 200,
            Budget::Normal => 500,
            Budget::Deep => 2000,
        }
    }

    /// Approximate total token budget for a response at this tier.
    pub fn total_budget_tokens(self) -> usize {
        match self {
            Budget::Tiny => 1_000,
            Budget::Normal => 3_000,
            Budget::Deep => 10_000,
        }
    }
}

/// Which store a field in a card response came from.
///
/// Every field in every card response is tagged with this so the agent
/// can reason about what it trusts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStore {
    /// From the canonical graph (parser_observed, human_declared, git_observed).
    Graph,
    /// From the overlay (machine_authored_*). Not present in phase 0/1 cards.
    Overlay,
}

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
    /// Drift flag summary across edges incident to this file.
    pub drift_flag: Option<String>,
    /// Approximate token count.
    pub approx_tokens: usize,
    /// Source store.
    pub source_store: SourceStore,
}

// Other card types (ModuleCard, EntryPointCard, CallPathCard,
// ChangeRiskCard, PublicAPICard, TestSurfaceCard, DecisionCard) follow the
// same pattern. Omitted from the skeleton to keep it focused.

/// Trait for compiling cards from the graph store.
pub trait CardCompiler {
    /// Compile a SymbolCard at the given budget.
    fn symbol_card(&self, id: SymbolNodeId, budget: Budget) -> crate::Result<SymbolCard>;

    /// Compile a FileCard at the given budget.
    fn file_card(&self, id: FileNodeId, budget: Budget) -> crate::Result<FileCard>;

    /// Resolve a human-readable target string (a path, a qualified name,
    /// or a symbol name) to a NodeId for card compilation.
    fn resolve_target(&self, target: &str) -> crate::Result<Option<NodeId>>;
}
