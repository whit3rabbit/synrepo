//! Node types for the canonical graph.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::core::ids::{ConceptNodeId, FileNodeId, SymbolNodeId};
use crate::core::provenance::Provenance;

use super::epistemic::Epistemic;

/// Visibility of a symbol node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Exported / externally callable.
    Public,
    /// Visible to the same compilation unit (e.g. `pub(crate)` in Rust).
    Crate,
    /// File or module-scoped.
    Private,
    /// Extraction could not determine.
    #[default]
    Unknown,
}

impl Visibility {
    /// Stable snake_case label used for ID derivation and persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Crate => "crate",
            Visibility::Private => "private",
            Visibility::Unknown => "unknown",
        }
    }

    /// Reverse of `as_str`: parse a stable snake_case label back to a `Visibility`.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "public" => Some(Visibility::Public),
            "crate" => Some(Visibility::Crate),
            "private" => Some(Visibility::Private),
            "unknown" => Some(Visibility::Unknown),
            _ => None,
        }
    }
}

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
    /// A named type definition (e.g. Go type declarations).
    TypeDef,
    /// An interface type (e.g. Go interface declarations).
    Interface,
    /// A module or namespace.
    Module,
    /// A constant or static.
    Constant,
    /// A top-level exported item (when the language has explicit exports).
    Export,
}

impl SymbolKind {
    /// Stable snake_case label used for ID derivation and persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Trait => "trait",
            SymbolKind::Type => "type",
            SymbolKind::TypeDef => "type_def",
            SymbolKind::Interface => "interface",
            SymbolKind::Module => "module",
            SymbolKind::Constant => "constant",
            SymbolKind::Export => "export",
        }
    }

    /// Reverse of `as_str`: parse a stable snake_case label back to a `SymbolKind`.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "function" => Some(SymbolKind::Function),
            "method" => Some(SymbolKind::Method),
            "class" => Some(SymbolKind::Class),
            "trait" => Some(SymbolKind::Trait),
            "type" => Some(SymbolKind::Type),
            "type_def" => Some(SymbolKind::TypeDef),
            "interface" => Some(SymbolKind::Interface),
            "module" => Some(SymbolKind::Module),
            "constant" => Some(SymbolKind::Constant),
            "export" => Some(SymbolKind::Export),
            _ => None,
        }
    }
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
    /// Decision text extracted from `// DECISION:` (or language-equivalent)
    /// line comments. Empty when no markers are present.
    #[serde(default)]
    pub inline_decisions: Vec<String>,
    /// Compile revision at which this file was last observed by the structural
    /// compile. `None` for rows written before the graph-lifecycle-v1 migration.
    #[serde(default)]
    pub last_observed_rev: Option<u64>,
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
    /// Visibility scope (e.g. Public, Crate, Private).
    #[serde(default)]
    pub visibility: Visibility,
    /// Byte offsets into the file where the symbol body begins and ends.
    pub body_byte_range: (u32, u32),
    /// blake3 hash of the symbol's body bytes.
    pub body_hash: String,
    /// One-line signature (e.g. `pub fn parse_query(input: &str) -> Result<Query, ParseError>`).
    pub signature: Option<String>,
    /// Doc comment text, if any (extracted by the language's `extra.scm`).
    pub doc_comment: Option<String>,
    /// Oldest sampled commit where this symbol's qualified name appeared.
    /// A lower bound: the symbol may predate the sampling window.
    #[serde(default)]
    pub first_seen_rev: Option<String>,
    /// Newest sampled commit where this symbol's body_hash differs from the
    /// current value. `None` when no transition was found in the window.
    #[serde(default)]
    pub last_modified_rev: Option<String>,
    /// Compile revision at which this symbol was last observed by the
    /// structural compile. `None` for pre-migration rows.
    #[serde(default)]
    pub last_observed_rev: Option<u64>,
    /// Compile revision at which this symbol stopped being emitted by its
    /// owning file's parse pass. `None` while the symbol is active.
    #[serde(default)]
    pub retired_at_rev: Option<u64>,
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
    /// ADR/decision status from frontmatter (e.g. "Accepted", "Deprecated").
    /// Present when the frontmatter contains a `status:` key.
    #[serde(default)]
    pub status: Option<String>,
    /// Decision body extracted from the `## Decision` section (or full body
    /// if no such heading is found). Present only for ADR-style documents.
    #[serde(default)]
    pub decision_body: Option<String>,
    /// Compile revision at which this concept was last observed by the
    /// structural compile. `None` for pre-migration rows.
    #[serde(default)]
    pub last_observed_rev: Option<u64>,
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
    use super::{SymbolNode, Visibility};

    #[test]
    fn symbol_node_visibility_defaults_to_unknown_on_missing_field() {
        // Simulate JSON without the visibility field (pre-migration data).
        let json = r#"{
            "id": "sym_00000000000000000000000000000001",
            "file_id": "file_00000000000000000000000000000001",
            "qualified_name": "crate::foo",
            "display_name": "foo",
            "kind": "function",
            "body_byte_range": [0, 10],
            "body_hash": "abc123",
            "signature": null,
            "doc_comment": null,
            "first_seen_rev": null,
            "last_modified_rev": null,
            "last_observed_rev": null,
            "retired_at_rev": null,
            "epistemic": "parser_observed",
            "provenance": {
                "created_at": "1970-01-01T00:00:00+00:00",
                "source_revision": "rev",
                "created_by": "structural_pipeline",
                "pass": "parse",
                "source_artifacts": []
            }
        }"#;

        let node: SymbolNode = serde_json::from_str(json).unwrap();
        assert_eq!(node.visibility, Visibility::Unknown);
    }

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
