//! Node types for the canonical graph.

use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, SymbolNodeId};
use crate::core::provenance::Provenance;

use super::epistemic::Epistemic;

/// Kind of a symbol node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// A free function.
    Function,
    /// A method on a class or struct.
    Method,
    /// A class or struct.
    Class,
    /// A trait, interface, or protocol.
    Trait,
    /// A type alias or typedef.
    Type,
    /// A module or namespace.
    Module,
    /// A constant or static.
    Constant,
    /// A top-level exported item (when the language has explicit exports).
    Export,
}

/// A file node in the canonical graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileNode {
    /// Stable identifier.
    pub id: FileNodeId,
    /// Current path relative to the repo root.
    pub path: String,
    /// Previous paths the file has had, newest first. Appended on rename.
    pub path_history: Vec<String>,
    /// blake3 hash of the current content.
    pub content_hash: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Detected language, if supported by a tree-sitter grammar.
    pub language: Option<String>,
    /// Epistemic origin (always `ParserObserved` for files).
    pub epistemic: Epistemic,
    /// Provenance metadata.
    pub provenance: Provenance,
}

/// A symbol node in the canonical graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolNode {
    /// Stable identifier derived from `(file_id, qualified_name, kind, body_hash)`.
    pub id: SymbolNodeId,
    /// The file that defines this symbol.
    pub file_id: FileNodeId,
    /// Fully qualified name within the file (e.g. `MyClass::my_method`).
    pub qualified_name: String,
    /// Short display name (just the trailing component).
    pub display_name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Byte offsets into the file where the symbol body begins and ends.
    pub body_byte_range: (u32, u32),
    /// blake3 hash of the symbol's body bytes.
    pub body_hash: String,
    /// One-line signature (e.g. `pub fn parse_query(input: &str) -> Result<Query, ParseError>`).
    pub signature: Option<String>,
    /// Doc comment text, if any (extracted by the language's `extra.scm`).
    pub doc_comment: Option<String>,
    /// Epistemic origin (always `ParserObserved` for symbols).
    pub epistemic: Epistemic,
    /// Provenance metadata.
    pub provenance: Provenance,
}
