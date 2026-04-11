//! Node types for the canonical graph.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::core::ids::{ConceptNodeId, FileNodeId, SymbolNodeId};
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

/// A human-authored concept node in the canonical graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConceptNode {
    /// Stable identifier.
    pub id: ConceptNodeId,
    /// Source markdown path relative to the repo root.
    pub path: String,
    /// Human-authored title for the concept or decision.
    pub title: String,
    /// Alternate names or short handles that refer to the same concept.
    pub aliases: Vec<String>,
    /// Optional one-line summary extracted from the source document.
    pub summary: Option<String>,
    /// Epistemic origin (always `HumanDeclared` for concept nodes).
    pub epistemic: Epistemic,
    /// Provenance metadata.
    pub provenance: Provenance,
}

/// Return true when a repo-relative path is eligible to produce a concept node.
pub fn concept_source_path_allowed(path: &str, concept_directories: &[String]) -> bool {
    let normalized = path.trim_start_matches("./");
    let candidate = Path::new(normalized);
    let Some(extension) = candidate.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    if !matches!(extension, "md" | "mdx" | "markdown") {
        return false;
    }

    concept_directories.iter().any(|directory| {
        let normalized_dir = directory.trim_matches('/');
        normalized == normalized_dir
            || (normalized.starts_with(normalized_dir)
                && normalized.as_bytes().get(normalized_dir.len()) == Some(&b'/'))
    })
}

#[cfg(test)]
mod tests {
    use super::concept_source_path_allowed;

    #[test]
    fn concept_source_paths_must_live_in_configured_markdown_dirs() {
        let dirs = vec!["docs/adr".to_string(), "architecture/decisions".to_string()];

        assert!(concept_source_path_allowed(
            "docs/adr/0001-record.md",
            &dirs
        ));
        assert!(concept_source_path_allowed(
            "./architecture/decisions/why-this.mdx",
            &dirs
        ));
        assert!(!concept_source_path_allowed(
            "docs/notes/0001-record.md",
            &dirs
        ));
        assert!(!concept_source_path_allowed(
            "docs/adr/0001-record.txt",
            &dirs
        ));
        assert!(!concept_source_path_allowed("docs/adr.md", &dirs));
    }
}
