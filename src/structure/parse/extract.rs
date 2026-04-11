use std::path::Path;

use tree_sitter::StreamingIterator as _;

use crate::structure::graph::SymbolKind;

use super::{ExtractedSymbol, Language, ParseOutput};

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
    let Some(language) = Language::from_extension(ext) else {
        return Ok(None);
    };

    let ts_language = language.tree_sitter_language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_language)
        .map_err(|error| crate::Error::Parse {
            path: path.display().to_string(),
            message: format!("failed to set language: {error}"),
        })?;

    let Some(tree) = parser.parse(content, None) else {
        return Ok(Some(ParseOutput {
            language,
            symbols: vec![],
            edges: vec![],
        }));
    };

    let query =
        tree_sitter::Query::new(&ts_language, language.definition_query()).map_err(|error| {
            crate::Error::Parse {
                path: path.display().to_string(),
                message: format!("query compilation failed: {error}"),
            }
        })?;

    let capture_names = query.capture_names();
    let item_idx = capture_names
        .iter()
        .position(|name| *name == "item")
        .map(|idx| idx as u32);
    let name_idx = capture_names
        .iter()
        .position(|name| *name == "name")
        .map(|idx| idx as u32);
    let (Some(item_idx), Some(name_idx)) = (item_idx, name_idx) else {
        return Ok(Some(ParseOutput {
            language,
            symbols: vec![],
            edges: vec![],
        }));
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut cursor_matches = cursor.matches(&query, tree.root_node(), content);
    let mut symbols = Vec::new();

    while let Some(query_match) = cursor_matches.next() {
        let item_node = query_match
            .captures
            .iter()
            .find(|capture| capture.index == item_idx)
            .map(|capture| capture.node);
        let name_node = query_match
            .captures
            .iter()
            .find(|capture| capture.index == name_idx)
            .map(|capture| capture.node);

        let (Some(item_node), Some(name_node)) = (item_node, name_node) else {
            continue;
        };

        let name_bytes = &content[name_node.start_byte()..name_node.end_byte()];
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };

        let base_kind = language.kind_for_pattern(query_match.pattern_index);
        let (qualified_name, kind) =
            build_qualified_name_and_kind(item_node, name, content, base_kind);

        let body_range = (item_node.start_byte() as u32, item_node.end_byte() as u32);
        let body_bytes = &content[item_node.start_byte()..item_node.end_byte()];

        symbols.push(ExtractedSymbol {
            qualified_name,
            display_name: name.to_string(),
            kind,
            body_byte_range: body_range,
            body_hash: hex::encode(blake3::hash(body_bytes).as_bytes()),
            signature: None,
            doc_comment: None,
        });
    }

    Ok(Some(ParseOutput {
        language,
        symbols,
        edges: vec![],
    }))
}

/// Walk up the parent chain to determine qualified name and kind.
///
/// For Rust functions inside an `impl` block, the kind becomes `Method` and
/// the qualified name is prefixed with the impl type (for example `MyStruct::foo`).
/// For Python methods inside a `class_definition`, the same rule applies.
fn build_qualified_name_and_kind(
    node: tree_sitter::Node,
    name: &str,
    source: &[u8],
    base_kind: SymbolKind,
) -> (String, SymbolKind) {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        match parent.kind() {
            "impl_item" => {
                if let Some(type_node) = parent.child_by_field_name("type") {
                    let type_bytes = &source[type_node.start_byte()..type_node.end_byte()];
                    if let Ok(type_name) = std::str::from_utf8(type_bytes) {
                        let base = type_name.split('<').next().unwrap_or(type_name).trim();
                        return (format!("{base}::{name}"), SymbolKind::Method);
                    }
                }
            }
            "block" => {
                if let Some(grandparent) = parent.parent() {
                    if grandparent.kind() == "class_definition" {
                        if let Some(class_name_node) = grandparent.child_by_field_name("name") {
                            let class_bytes =
                                &source[class_name_node.start_byte()..class_name_node.end_byte()];
                            if let Ok(class_name) = std::str::from_utf8(class_bytes) {
                                return (format!("{class_name}::{name}"), SymbolKind::Method);
                            }
                        }
                    }
                }
            }
            "class_body" => {
                if let Some(grandparent) = parent.parent() {
                    if matches!(grandparent.kind(), "class_declaration" | "class") {
                        if let Some(class_name_node) = grandparent.child_by_field_name("name") {
                            let class_bytes =
                                &source[class_name_node.start_byte()..class_name_node.end_byte()];
                            if let Ok(class_name) = std::str::from_utf8(class_bytes) {
                                return (format!("{class_name}::{name}"), SymbolKind::Method);
                            }
                        }
                    }
                }
            }
            "source_file" | "program" => break,
            _ => {}
        }
        ancestor = parent.parent();
    }

    (name.to_string(), base_kind)
}
