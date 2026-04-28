//! Tree-sitter parsing and symbol extraction.
//!
//! Parses supported source files and extracts `ExtractedSymbol` and
//! `ExtractedEdge` records consumed by the structural compile pipeline.
//! Within-file edges are returned, cross-file resolution is deferred to
//! pipeline stage 4 (not part of this initial producer set).
//!
//! ## Parser invariants (enforced in tests)
//!
//! 1. **Query compile contract.** Every `Language` variant in
//!    `Language::supported()` must have a `definition_query`, `call_query`,
//!    and `import_query` that compile against its grammar, and each must
//!    expose the required capture names (`item`/`name`, `callee`,
//!    `import_ref`). See the `validation_tests` module.
//! 2. **Pattern-index mapping.** `Language::kind_map()` pins
//!    pattern-index → `SymbolKind`. The table's length must equal the
//!    compiled definition query's `pattern_count()`, and every index must
//!    have an explicit slot — runtime keeps a `SymbolKind::Function`
//!    fallback for forward-compatibility, but tests pin the full mapping
//!    so drift fails CI loud.
//! 3. **Malformed-source semantics.** `parse_file` follows this contract:
//!    - Unsupported extension → `Ok(None)`.
//!    - Supported extension, tree-sitter parse fails outright → returns
//!      `Ok(Some(ParseOutput))` with empty symbol/edge/refs vectors.
//!    - Supported extension, syntactically malformed source → returns
//!      `Ok(Some(ParseOutput))` with deterministic best-effort extraction.
//!    - Empty input → returns `Ok(Some(ParseOutput))` with no symbols.
//!
//!    Runtime never panics on bad input and never escalates malformed
//!    user source to a hard error.
//! 4. **Stage-4-facing outputs.** `ParseOutput.call_refs` and
//!    `ParseOutput.import_refs` are first-class tested outputs. Stage 4
//!    treats unresolved entries as silent skips, so parser tests assert
//!    these fields directly rather than relying on downstream edge
//!    emission to surface regressions.

mod extract;
mod language;

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod malformed_tests;
#[cfg(test)]
mod qualname_tests;
#[cfg(test)]
mod refs_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_tests;

use crate::structure::graph::{EdgeKind, SymbolKind, Visibility};

/// Call mode classification for stage-4 scope narrowing.
///
/// Determines whether a call site is a free function call or a method/attribute call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallMode {
    /// Free function call: `foo()` without a receiver/qualifier.
    Free,
    /// Method or attribute call: `obj.method()` or `Type::method()`.
    Method,
}

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
    /// Visibility scope.
    pub visibility: Visibility,
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

/// Enclosing caller identity carried with a call reference.
///
/// The pair matches the symbol identity inputs available before stage 4 has
/// access to persisted `SymbolNodeId`s for the current compile.
#[derive(Clone, Debug)]
pub struct ExtractedCallerSymbol {
    /// Fully qualified caller symbol name within the file.
    pub qualified_name: String,
    /// Body hash paired with the qualified name to identify the exact symbol.
    pub body_hash: String,
}

/// A call site reference extracted during parse for stage-4 resolution.
///
/// The callee name is the local name as it appears at the call site. Stage 4
/// resolves it against the global symbol name index and uses the optional
/// `callee_prefix` plus `is_method` flag for scope-narrowing; unresolved names
/// are silently skipped (approximate resolution is acceptable in phase 1).
#[derive(Clone, Debug)]
pub struct ExtractedCallRef {
    /// Name of the called function or method (local, not fully qualified).
    pub callee_name: String,
    /// Receiver or qualifier text at the call site (`"foo"` for `foo.bar()`,
    /// `"Type"` for `Type::method()`, `None` for a bare `bar()`).
    pub callee_prefix: Option<String>,
    /// True for method/attribute calls (`obj.method()`), false for free calls.
    pub is_method: bool,
    /// Enclosing caller symbol, absent for module-scope call sites.
    pub caller: Option<ExtractedCallerSymbol>,
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
