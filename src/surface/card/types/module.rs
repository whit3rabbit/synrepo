use serde::{Deserialize, Serialize};

use super::{FileRef, SourceStore, SymbolRef};

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
