//! Tree-sitter parsing and symbol extraction.
//!
//! Per the reuse strategy in `synrepo-design-v4.md`, this module reads
//! `LOCALS_QUERY` and related constants **directly from the per-language
//! tree-sitter crates** (which ship them as `&'static str`) and merges in
//! small per-language `extra.scm` files shipped under
//! `src/parse/extra/<lang>/extra.scm` for synrepo-specific captures.

use crate::structure::graph::SymbolKind;
use std::path::Path;

/// Supported languages with a tree-sitter grammar wired in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    /// Rust (`tree-sitter-rust` crate).
    Rust,
    /// Python (`tree-sitter-python` crate).
    Python,
    /// TypeScript, non-TSX (`tree-sitter-typescript::language_typescript`).
    TypeScript,
    /// TypeScript with JSX (`tree-sitter-typescript::language_tsx`).
    Tsx,
}

impl Language {
    /// Resolve a file extension to a `Language`, if supported.
    pub fn from_extension(ext: &str) -> Option<Language> {
        match ext {
            "rs" => Some(Language::Rust),
            "py" => Some(Language::Python),
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            _ => None,
        }
    }

    /// Return the tree-sitter `Language` from the corresponding crate.
    pub fn tree_sitter_language(self) -> tree_sitter::Language {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    /// Load the `LOCALS_QUERY` string from the language crate, plus any
    /// synrepo-specific `extra.scm` captures for this language.
    ///
    /// Phase 1 TODO: actually read the bundled constants and merge.
    /// The per-language crates expose these as `LOCALS_QUERY: &str`,
    /// `HIGHLIGHTS_QUERY: &str`, etc. at the crate root. Some older or
    /// community-maintained crates may lag — pin versions and CI against
    /// representative source files per language.
    pub fn merged_query_source(self) -> &'static str {
        // TODO(phase-1):
        //   let bundled = tree_sitter_rust::LOCALS_QUERY; // or similar
        //   let extra = include_str!("parse/extra/rust/extra.scm");
        //   concat!(bundled, "\n", extra)
        ""
    }
}

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
    /// Target — may refer to a symbol in another file; resolution is deferred.
    pub to_reference: String,
    /// Kind of edge observed.
    pub kind: crate::structure::graph::EdgeKind,
}

/// Result of parsing one source file.
pub struct ParseOutput {
    /// Language identified.
    pub language: Language,
    /// Symbols defined in this file.
    pub symbols: Vec<ExtractedSymbol>,
    /// Edges observed within this file.
    pub edges: Vec<ExtractedEdge>,
}

/// Parse a source file and extract symbols and within-file edges.
///
/// Phase 1 TODO: wire up tree-sitter, run the merged query, walk the tree,
/// extract per-kind captures into `ExtractedSymbol`, compute body hashes.
pub fn parse_file(path: &Path, _content: &[u8]) -> crate::Result<Option<ParseOutput>> {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return Ok(None);
    };
    let Some(_lang) = Language::from_extension(ext) else {
        return Ok(None);
    };
    // TODO(phase-1): actually parse.
    Ok(None)
}
