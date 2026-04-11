//! Tree-sitter parsing and symbol extraction.
//!
//! Parses supported source files and extracts `ExtractedSymbol` and
//! `ExtractedEdge` records consumed by the structural compile pipeline.
//! Within-file edges are returned; cross-file resolution is deferred to
//! pipeline stage 4 (not part of this initial producer set).

use crate::structure::graph::{EdgeKind, SymbolKind};
use std::path::Path;
use tree_sitter::StreamingIterator as _;

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

    /// Tree-sitter query string capturing named definition nodes.
    ///
    /// Each pattern captures `@item` (the whole definition) and `@name`
    /// (the identifier node). Pattern indices map to `kind_for_pattern`.
    fn definition_query(self) -> &'static str {
        match self {
            Language::Rust => RUST_DEFINITION_QUERY,
            Language::Python => PYTHON_DEFINITION_QUERY,
            Language::TypeScript | Language::Tsx => TS_DEFINITION_QUERY,
        }
    }

    /// Map a pattern index from `definition_query` to a `SymbolKind`.
    fn kind_for_pattern(self, pattern_index: usize) -> SymbolKind {
        match self {
            Language::Rust => RUST_KIND_MAP
                .get(pattern_index)
                .copied()
                .unwrap_or(SymbolKind::Function),
            Language::Python => PYTHON_KIND_MAP
                .get(pattern_index)
                .copied()
                .unwrap_or(SymbolKind::Function),
            Language::TypeScript | Language::Tsx => TS_KIND_MAP
                .get(pattern_index)
                .copied()
                .unwrap_or(SymbolKind::Function),
        }
    }
}

// Capture top-level items and items inside impl blocks (methods).
// Pattern order must match RUST_KIND_MAP.
const RUST_DEFINITION_QUERY: &str = r#"
(function_item name: (identifier) @name) @item
(struct_item name: (type_identifier) @name) @item
(enum_item name: (type_identifier) @name) @item
(trait_item name: (type_identifier) @name) @item
(type_item name: (type_identifier) @name) @item
(mod_item name: (identifier) @name) @item
(const_item name: (identifier) @name) @item
(static_item name: (identifier) @name) @item
"#;

const RUST_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function, // function_item (may become Method if inside impl)
    SymbolKind::Class,    // struct_item
    SymbolKind::Class,    // enum_item
    SymbolKind::Trait,    // trait_item
    SymbolKind::Type,     // type_item
    SymbolKind::Module,   // mod_item
    SymbolKind::Constant, // const_item
    SymbolKind::Constant, // static_item
];

const PYTHON_DEFINITION_QUERY: &str = r#"
(function_definition name: (identifier) @name) @item
(class_definition name: (identifier) @name) @item
"#;

const PYTHON_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
];

// TypeScript / TSX share the same grammar entry points for top-level items.
const TS_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(class_declaration name: (type_identifier) @name) @item
(interface_declaration name: (type_identifier) @name) @item
(type_alias_declaration name: (type_identifier) @name) @item
(method_definition name: (property_identifier) @name) @item
(abstract_method_signature name: (property_identifier) @name) @item
"#;

const TS_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Trait,    // interface
    SymbolKind::Type,     // type alias
    SymbolKind::Method,
    SymbolKind::Method,   // abstract method
];

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
    pub kind: EdgeKind,
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
/// Returns `None` if the file extension is not supported by any wired grammar.
/// Returns `Some(ParseOutput)` (possibly with empty symbol list) otherwise.
/// Parse errors inside tree-sitter are treated as partial results rather than
/// hard failures, because syntax errors in the source file should not prevent
/// the rest of the graph from being populated.
pub fn parse_file(path: &Path, content: &[u8]) -> crate::Result<Option<ParseOutput>> {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return Ok(None);
    };
    let Some(lang) = Language::from_extension(ext) else {
        return Ok(None);
    };

    let ts_lang = lang.tree_sitter_language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| crate::Error::Parse {
            path: path.display().to_string(),
            message: format!("failed to set language: {e}"),
        })?;

    // tree-sitter produces a partial tree even for files with syntax errors.
    let Some(tree) = parser.parse(content, None) else {
        return Ok(Some(ParseOutput {
            language: lang,
            symbols: vec![],
            edges: vec![],
        }));
    };

    let query_src = lang.definition_query();
    let query = tree_sitter::Query::new(&ts_lang, query_src).map_err(|e| {
        crate::Error::Parse {
            path: path.display().to_string(),
            message: format!("query compilation failed: {e}"),
        }
    })?;

    // Locate the capture indices for @item and @name.
    let capture_names = query.capture_names();
    let item_idx = capture_names
        .iter()
        .position(|n| *n == "item")
        .map(|i| i as u32);
    let name_idx = capture_names
        .iter()
        .position(|n| *n == "name")
        .map(|i| i as u32);
    let (Some(item_idx), Some(name_idx)) = (item_idx, name_idx) else {
        return Ok(Some(ParseOutput {
            language: lang,
            symbols: vec![],
            edges: vec![],
        }));
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut cursor_matches = cursor.matches(&query, tree.root_node(), content);

    let mut symbols = Vec::new();

    while let Some(m) = cursor_matches.next() {
        let item_node = m.captures.iter().find(|c| c.index == item_idx).map(|c| c.node);
        let name_node = m.captures.iter().find(|c| c.index == name_idx).map(|c| c.node);

        let (Some(item_node), Some(name_node)) = (item_node, name_node) else {
            continue;
        };

        let name_bytes: &[u8] = &content[name_node.start_byte()..name_node.end_byte()];
        let Ok(name_str) = std::str::from_utf8(name_bytes) else {
            continue;
        };

        let base_kind = lang.kind_for_pattern(m.pattern_index);
        let (qualified_name, kind) =
            build_qualified_name_and_kind(item_node, name_str, content, base_kind);

        let start = item_node.start_byte() as u32;
        let end = item_node.end_byte() as u32;
        let body_bytes: &[u8] = &content[item_node.start_byte()..item_node.end_byte()];
        let body_hash = hex::encode(blake3::hash(body_bytes).as_bytes());

        symbols.push(ExtractedSymbol {
            qualified_name,
            display_name: name_str.to_string(),
            kind,
            body_byte_range: (start, end),
            body_hash,
            signature: None,  // TODO(phase-1): extract first-line signature
            doc_comment: None, // TODO(phase-1): extract doc comment
        });
    }

    Ok(Some(ParseOutput {
        language: lang,
        symbols,
        edges: vec![], // Cross-file edges deferred to pipeline stage 4.
    }))
}

/// Walk up the parent chain to determine qualified name and kind.
///
/// For Rust functions inside an `impl` block, the kind becomes `Method` and
/// the qualified name is prefixed with the impl type (e.g. `MyStruct::foo`).
/// For Python methods inside a `class_definition`, the same rule applies.
fn build_qualified_name_and_kind(
    node: tree_sitter::Node,
    name: &str,
    source: &[u8],
    base_kind: SymbolKind,
) -> (String, SymbolKind) {
    let mut ancestor = node.parent();
    while let Some(p) = ancestor {
        match p.kind() {
            // Rust impl block
            "impl_item" => {
                if let Some(type_node) = p.child_by_field_name("type") {
                    let type_bytes: &[u8] = &source[type_node.start_byte()..type_node.end_byte()];
                    if let Ok(type_name) = std::str::from_utf8(type_bytes) {
                        // Strip generic parameters for the qualified prefix.
                        let base = type_name
                            .split('<')
                            .next()
                            .unwrap_or(type_name)
                            .trim();
                        return (
                            format!("{base}::{name}"),
                            SymbolKind::Method,
                        );
                    }
                }
            }
            // Python class body
            "block" => {
                if let Some(grandparent) = p.parent() {
                    if grandparent.kind() == "class_definition" {
                        if let Some(class_name_node) =
                            grandparent.child_by_field_name("name")
                        {
                            let cn_bytes: &[u8] = &source[class_name_node.start_byte()..class_name_node.end_byte()];
                            if let Ok(class_name) = std::str::from_utf8(cn_bytes) {
                                return (
                                    format!("{class_name}::{name}"),
                                    SymbolKind::Method,
                                );
                            }
                        }
                    }
                }
            }
            // TypeScript / TSX class body
            "class_body" => {
                if let Some(grandparent) = p.parent() {
                    if matches!(grandparent.kind(), "class_declaration" | "class") {
                        if let Some(class_name_node) =
                            grandparent.child_by_field_name("name")
                        {
                            let cn_bytes: &[u8] = &source[class_name_node.start_byte()..class_name_node.end_byte()];
                            if let Ok(class_name) = std::str::from_utf8(cn_bytes)
                            {
                                return (
                                    format!("{class_name}::{name}"),
                                    SymbolKind::Method,
                                );
                            }
                        }
                    }
                }
            }
            // Stop climbing at file-level boundaries.
            "source_file" | "program" => break,
            _ => {}
        }
        ancestor = p.parent();
    }
    (name.to_string(), base_kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parse_file_extracts_rust_top_level_definitions() {
        let source = b"
pub fn greet(name: &str) -> String {
    format!(\"Hello, {name}!\")
}

pub struct Greeter {
    name: String,
}

pub trait Greetable {
    fn greet(&self) -> String;
}

pub enum Status {
    Active,
    Inactive,
}

pub type Name = String;

pub mod helpers {}

pub const MAX: usize = 100;
";
        let output = parse_file(Path::new("src/lib.rs"), source)
            .unwrap()
            .unwrap();

        assert_eq!(output.language, Language::Rust);
        let names: Vec<&str> = output.symbols.iter().map(|s| s.display_name.as_str()).collect();
        assert!(names.contains(&"greet"), "expected greet, got: {names:?}");
        assert!(names.contains(&"Greeter"), "expected Greeter, got: {names:?}");
        assert!(names.contains(&"Greetable"), "expected Greetable, got: {names:?}");
        assert!(names.contains(&"Status"), "expected Status, got: {names:?}");
        assert!(names.contains(&"Name"), "expected Name, got: {names:?}");
        assert!(names.contains(&"helpers"), "expected helpers, got: {names:?}");
        assert!(names.contains(&"MAX"), "expected MAX, got: {names:?}");
    }

    #[test]
    fn parse_file_qualifies_rust_impl_methods() {
        let source = b"
pub struct Calculator {}

impl Calculator {
    pub fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}
";
        let output = parse_file(Path::new("src/calc.rs"), source)
            .unwrap()
            .unwrap();

        let method = output
            .symbols
            .iter()
            .find(|s| s.display_name == "add")
            .expect("add method not found");

        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.qualified_name, "Calculator::add");
    }

    #[test]
    fn parse_file_returns_none_for_unsupported_extension() {
        assert!(
            parse_file(Path::new("config.yaml"), b"key: val").unwrap().is_none()
        );
        assert!(parse_file(Path::new("README.md"), b"# hi").unwrap().is_none());
    }

    #[test]
    fn parse_file_returns_empty_symbols_for_empty_rust_file() {
        let output = parse_file(Path::new("src/empty.rs"), b"")
            .unwrap()
            .unwrap();
        assert!(output.symbols.is_empty());
    }

    #[test]
    fn parse_file_body_hash_is_stable_for_same_content() {
        let source = b"pub fn foo() {}";
        let out1 = parse_file(Path::new("a.rs"), source).unwrap().unwrap();
        let out2 = parse_file(Path::new("b.rs"), source).unwrap().unwrap();
        assert_eq!(out1.symbols[0].body_hash, out2.symbols[0].body_hash);
    }

    #[test]
    fn parse_file_extracts_python_functions_and_classes() {
        let source = b"
def greet(name):
    return f'Hello, {name}'

class Greeter:
    def greet(self):
        return 'hi'
";
        let output = parse_file(Path::new("app.py"), source)
            .unwrap()
            .unwrap();

        let names: Vec<&str> = output.symbols.iter().map(|s| s.display_name.as_str()).collect();
        assert!(names.contains(&"greet"), "expected greet in {names:?}");
        assert!(names.contains(&"Greeter"), "expected Greeter in {names:?}");
    }
}
