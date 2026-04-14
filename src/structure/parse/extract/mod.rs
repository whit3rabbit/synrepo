mod docs;
mod qualname;

use std::path::Path;

use tree_sitter::StreamingIterator as _;

use super::{ExtractedCallRef, ExtractedImportRef, ExtractedSymbol, Language, ParseOutput};

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
            call_refs: vec![],
            import_refs: vec![],
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
            call_refs: vec![],
            import_refs: vec![],
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

        let name = node_text(name_node, content);
        if name.is_empty() {
            continue;
        }

        let base_kind = language.kind_for_pattern(query_match.pattern_index);
        let (qualified_name, kind) =
            qualname::build_qualified_name_and_kind(item_node, &name, content, base_kind);

        let body_range = (item_node.start_byte() as u32, item_node.end_byte() as u32);
        let body_bytes = &content[item_node.start_byte()..item_node.end_byte()];

        symbols.push(ExtractedSymbol {
            qualified_name,
            display_name: name,
            kind,
            body_byte_range: body_range,
            body_hash: hex::encode(blake3::hash(body_bytes).as_bytes()),
            signature: docs::extract_signature(item_node, content, language),
            doc_comment: docs::extract_doc_comment(item_node, content, language),
        });
    }

    let call_refs = extract_call_refs(language, &ts_language, &tree, content, path)?;
    let import_refs = extract_import_refs(language, &ts_language, &tree, content, path)?;

    Ok(Some(ParseOutput {
        language,
        symbols,
        edges: vec![],
        call_refs,
        import_refs,
    }))
}

/// Extract call-site references from a parsed file for stage-4 resolution.
fn extract_call_refs(
    language: Language,
    ts_language: &tree_sitter::Language,
    tree: &tree_sitter::Tree,
    content: &[u8],
    path: &Path,
) -> crate::Result<Vec<ExtractedCallRef>> {
    let query = tree_sitter::Query::new(ts_language, language.call_query()).map_err(|e| {
        crate::Error::Parse {
            path: path.display().to_string(),
            message: format!("call query compilation failed: {e}"),
        }
    })?;

    let capture_names = query.capture_names();
    let callee_idx = capture_names
        .iter()
        .position(|n| *n == "callee")
        .map(|i| i as u32);
    let Some(callee_idx) = callee_idx else {
        return Ok(vec![]);
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), content);
    let mut refs = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures.iter().filter(|c| c.index == callee_idx) {
            let name = node_text(capture.node, content);
            if !name.is_empty() {
                refs.push(ExtractedCallRef { callee_name: name });
            }
        }
    }

    Ok(refs)
}

/// Extract import/use references from a parsed file for stage-4 resolution.
fn extract_import_refs(
    language: Language,
    ts_language: &tree_sitter::Language,
    tree: &tree_sitter::Tree,
    content: &[u8],
    path: &Path,
) -> crate::Result<Vec<ExtractedImportRef>> {
    let query = tree_sitter::Query::new(ts_language, language.import_query()).map_err(|e| {
        crate::Error::Parse {
            path: path.display().to_string(),
            message: format!("import query compilation failed: {e}"),
        }
    })?;

    let capture_names = query.capture_names();
    let ref_idx = capture_names
        .iter()
        .position(|n| *n == "import_ref")
        .map(|i| i as u32);
    let Some(ref_idx) = ref_idx else {
        return Ok(vec![]);
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), content);
    let mut refs = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures.iter().filter(|c| c.index == ref_idx) {
            let module_ref = node_text(capture.node, content);
            if !module_ref.is_empty() {
                refs.push(ExtractedImportRef { module_ref });
            }
        }
    }

    Ok(refs)
}

/// Extract the raw text of a tree-sitter node from the source buffer.
fn node_text(node: tree_sitter::Node, source: &[u8]) -> String {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
        .unwrap_or("")
        .to_string()
}
