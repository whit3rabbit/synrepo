use serde::{Deserialize, Serialize};

use super::{ContextAccounting, SourceStore, SymbolRef};

/// A single edge in a call path from entry point to target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallPathEdge {
    /// Source symbol (caller).
    pub from: SymbolRef,
    /// Target symbol (callee).
    pub to: SymbolRef,
    /// Kind of edge (always "Calls" for v1).
    pub edge_kind: String,
    /// Whether this path was truncated due to depth limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

/// A single call path from an entry point to the target symbol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallPath {
    /// The entry point symbol where this path starts.
    pub entry_point: SymbolRef,
    /// The target symbol at the end of this path.
    pub target: SymbolRef,
    /// Ordered list of edges from entry point to target.
    pub edges: Vec<CallPathEdge>,
    /// Number of additional paths omitted due to deduplication cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths_omitted: Option<usize>,
}

/// CallPathCard — answers "how do I reach this function from entry points?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallPathCard {
    /// The target symbol this card traces paths to.
    pub target: SymbolRef,
    /// All discovered call paths from entry points to the target.
    pub paths: Vec<CallPath>,
    /// Total count of omitted paths across all (entry_point, target) pairs.
    pub paths_omitted: usize,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Context-accounting metadata for this card.
    pub context_accounting: ContextAccounting,
    /// Source store (always `Graph` for call-path cards).
    pub source_store: SourceStore,
}
