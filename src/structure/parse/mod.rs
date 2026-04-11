//! Tree-sitter parsing and symbol extraction.
//!
//! Parses supported source files and extracts `ExtractedSymbol` and
//! `ExtractedEdge` records consumed by the structural compile pipeline.
//! Within-file edges are returned, cross-file resolution is deferred to
//! pipeline stage 4 (not part of this initial producer set).

mod extract;
mod language;

#[cfg(test)]
mod tests;

use crate::structure::graph::{EdgeKind, SymbolKind};

pub use extract::parse_file;
pub use language::Language;

/// A symbol the parser extracted from a source file.
#[derive(Clone, Debug)]
pub struct ExtractedSymbol {
    /// Fully qualified name within the file.
    pub qualified_name: String,
    /// Short display name.
    pub display_name: String,
    /// Kind.
    pub kind: SymbolKind,
    /// Byte offsets of the symbol body in the file.
    pub body_byte_range: (u32, u32),
    /// blake3 hash of the body bytes.
    pub body_hash: String,
    /// One-line signature, if extractable.
    pub signature: Option<String>,
    /// Doc comment, if extractable.
    pub doc_comment: Option<String>,
}

/// Edges the parser observed between symbols within this file (calls,
/// inherits, references, etc.). Cross-file edges are resolved later by
/// the pipeline once the whole compile cycle's symbols are in the graph.
#[derive(Clone, Debug)]
pub struct ExtractedEdge {
    /// Fully qualified name of the source symbol within this file.
    pub from_qualified_name: String,
    /// Target, may refer to a symbol in another file, resolution is deferred.
    pub to_reference: String,
    /// Kind of edge observed.
    pub kind: EdgeKind,
}

/// A call site reference extracted during parse for stage-4 resolution.
///
/// The callee name is the local name as it appears at the call site. Stage 4
/// resolves it against the global symbol name index; unresolved names are
/// silently skipped (approximate resolution is acceptable in phase 1).
#[derive(Clone, Debug)]
pub struct ExtractedCallRef {
    /// Name of the called function or method (local, not fully qualified).
    pub callee_name: String,
}

/// An import/use reference extracted during parse for stage-4 resolution.
///
/// The module_ref is the raw text captured from the import statement. Stage 4
/// resolves it to a FileNodeId where possible; unresolved refs are skipped.
#[derive(Clone, Debug)]
pub struct ExtractedImportRef {
    /// Raw module path or name as written in the source.
    pub module_ref: String,
}

/// Result of parsing one source file.
pub struct ParseOutput {
    /// Language identified.
    pub language: Language,
    /// Symbols defined in this file.
    pub symbols: Vec<ExtractedSymbol>,
    /// Edges observed within this file.
    pub edges: Vec<ExtractedEdge>,
    /// Call-site references for stage-4 cross-file Calls edge resolution.
    pub call_refs: Vec<ExtractedCallRef>,
    /// Import references for stage-4 cross-file Imports edge resolution.
    pub import_refs: Vec<ExtractedImportRef>,
}
