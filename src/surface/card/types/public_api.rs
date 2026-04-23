use serde::{Deserialize, Serialize};

use crate::core::ids::SymbolNodeId;
use crate::structure::graph::SymbolKind;

use super::super::git::SymbolLastChange;
use super::{ContextAccounting, SourceStore};

/// One exported symbol in a `PublicAPICard`.
///
/// Visibility is inferred from `signature`: if it starts with `pub`, the
/// symbol is considered exported. This heuristic works for Rust (`pub fn`,
/// `pub struct`, `pub(crate)`, etc.). For Python, TypeScript, and Go, where
/// visibility is not expressed as a `pub` keyword, `public_symbols` will be
/// empty in v1.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicAPIEntry {
    /// Stable node ID of the symbol.
    pub id: SymbolNodeId,
    /// Short display name.
    pub name: String,
    /// Symbol kind (function, struct, trait, etc.).
    pub kind: SymbolKind,
    /// Full declaration prefix, e.g. `pub fn parse(input: &str) -> Result<…>`.
    /// The `pub` prefix is the visibility signal; callers may inspect it directly.
    pub signature: String,
    /// `"path:byte_offset"` for IDE navigation.
    pub location: String,
    /// Most recent change for this symbol's containing file.
    /// Absent at `Tiny`; present at `Normal` and `Deep`.
    /// At `Deep`, includes a human-readable summary string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<SymbolLastChange>,
}

/// `PublicAPICard` — answers "what does this module/crate expose?"
///
/// Surfaces the exported API of a directory: public symbols with kinds and
/// signatures, public entry points (the subset also detected as execution
/// entry points), and (at `Deep` budget) symbols whose containing file was
/// last touched within 30 days.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicAPICard {
    /// Directory path this card describes (normalised with trailing `/`).
    pub path: String,
    /// Public symbols from direct-child files.
    /// Empty at `Tiny`; populated at `Normal` and `Deep`.
    pub public_symbols: Vec<PublicAPIEntry>,
    /// Count of all public symbols across direct-child files (always present).
    pub public_symbol_count: usize,
    /// Subset of `public_symbols` also classified as execution entry points.
    /// Empty at `Tiny`; populated at `Normal` and `Deep`.
    pub public_entry_points: Vec<PublicAPIEntry>,
    /// Public symbols whose containing file was last touched within 30 days.
    /// Only populated at `Deep` budget; omitted from JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_api_changes: Vec<PublicAPIEntry>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Context-accounting metadata for this card.
    pub context_accounting: ContextAccounting,
    /// Source store (always `Graph` for public-API cards).
    pub source_store: SourceStore,
}
