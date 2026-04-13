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
            signature: extract_signature(item_node, content, language),
            doc_comment: extract_doc_comment(item_node, content, language),
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

/// Extract the doc comment for a definition node by language-specific strategy.
///
/// Rust: walk preceding siblings collecting contiguous `///` line comments.
/// Python: read the first statement of the body if it is a string literal.
/// TypeScript/TSX: find the nearest preceding `/**` block comment.
fn extract_doc_comment(
    item_node: tree_sitter::Node,
    source: &[u8],
    language: Language,
) -> Option<String> {
    match language {
        Language::Rust => {
            let mut lines: Vec<String> = Vec::new();
            let mut prev = item_node.prev_named_sibling();
            while let Some(node) = prev {
                match node.kind() {
                    "line_comment" => {
                        let t = node_text(node, source);
                        match t.strip_prefix("///") {
                            Some(rest) => lines.push(rest.trim().to_string()),
                            None => break,
                        }
                    }
                    "attribute_item" => {} // skip #[…] attributes between doc and item
                    _ => break,
                }
                prev = node.prev_named_sibling();
            }
            if lines.is_empty() {
                return None;
            }
            lines.reverse();
            Some(lines.join("\n"))
        }
        Language::Python => {
            let body = item_node.child_by_field_name("body")?;
            let first = body.named_child(0)?;
            if first.kind() != "expression_statement" {
                return None;
            }
            let sn = first.named_child(0)?;
            if sn.kind() != "string" {
                return None;
            }
            strip_python_quotes(&node_text(sn, source))
        }
        Language::TypeScript | Language::Tsx => {
            let mut prev = item_node.prev_named_sibling();
            while let Some(node) = prev {
                if node.kind() == "decorator" {
                    prev = node.prev_named_sibling();
                    continue;
                }
                if node.kind() == "comment" {
                    let t = node_text(node, source);
                    if t.starts_with("/**") {
                        let c = strip_jsdoc(&t);
                        return if c.is_empty() { None } else { Some(c) };
                    }
                }
                break;
            }
            None
        }
        Language::Go => {
            // Go doc comments are contiguous `//` or `/* */` comment nodes
            // immediately preceding the declaration. Unlike Rust, no special
            // prefix is required: any preceding comment is a doc comment.
            let mut lines: Vec<String> = Vec::new();
            let mut prev = item_node.prev_named_sibling();
            while let Some(node) = prev {
                if node.kind() != "comment" {
                    break;
                }
                let t = node_text(node, source);
                let stripped = if let Some(rest) = t.strip_prefix("// ") {
                    rest.trim_end().to_string()
                } else if let Some(rest) = t.strip_prefix("//") {
                    rest.trim().to_string()
                } else if t.starts_with("/*") {
                    strip_comment_block(&t, "/*")
                } else {
                    break;
                };
                lines.push(stripped);
                prev = node.prev_named_sibling();
            }
            if lines.is_empty() {
                return None;
            }
            lines.reverse();
            let joined = lines.join("\n");
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        }
    }
}

/// Extract the declaration signature for a definition node.
///
/// Returns the text from the start of the node to the first body delimiter
/// (`{` or `;` for Rust, `:` at depth-0 for Python, `{` for TypeScript/TSX),
/// with internal whitespace collapsed to a single space and capped at 200 chars.
fn extract_signature(
    item_node: tree_sitter::Node,
    source: &[u8],
    language: Language,
) -> Option<String> {
    let text = node_text(item_node, source);
    let sig = match language {
        Language::Rust => {
            let end = text.find(['{', ';']).unwrap_or(text.len());
            collapse_ws(&text[..end])
        }
        Language::Python => {
            let mut depth: i32 = 0;
            let end = text
                .char_indices()
                .find_map(|(i, c)| match c {
                    '(' | '[' => {
                        depth += 1;
                        None
                    }
                    ')' | ']' => {
                        depth -= 1;
                        None
                    }
                    ':' if depth == 0 => Some(i),
                    _ => None,
                })
                .unwrap_or(text.len());
            collapse_ws(&text[..end])
        }
        Language::TypeScript | Language::Tsx => {
            // Arrow functions: take only the LHS up to `=`.
            if matches!(
                item_node.kind(),
                "variable_declaration" | "lexical_declaration"
            ) {
                if let Some(p) = text.find('=') {
                    return Some(collapse_ws(&text[..p]));
                }
            }
            let end = text.find('{').unwrap_or(text.len());
            collapse_ws(&text[..end])
        }
        Language::Go => {
            // For functions, methods, structs, and interfaces: take up to the
            // opening `{`. For constants/variables: take the full text.
            let end = text.find('{').unwrap_or(text.len());
            collapse_ws(&text[..end])
        }
    };
    if sig.is_empty() {
        None
    } else {
        Some(sig)
    }
}

fn node_text(node: tree_sitter::Node, source: &[u8]) -> String {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
        .unwrap_or("")
        .to_string()
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(210));
    for (i, word) in s.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(word);
    }
    // Truncate by char count, not bytes, to avoid splitting multi-byte UTF-8 sequences.
    if out.chars().count() > 200 {
        let truncated: String = out.chars().take(200).collect();
        format!("{truncated}…")
    } else {
        out
    }
}

fn strip_python_quotes(raw: &str) -> Option<String> {
    let r = raw.trim();
    let c = if (r.starts_with("\"\"\"") && r.ends_with("\"\"\""))
        || (r.starts_with("'''") && r.ends_with("'''"))
    {
        if r.len() < 6 {
            return None;
        }
        r[3..r.len() - 3].trim().to_string()
    } else if r.len() >= 2
        && ((r.starts_with('"') && r.ends_with('"')) || (r.starts_with('\'') && r.ends_with('\'')))
    {
        r[1..r.len() - 1].to_string()
    } else {
        return None;
    };
    if c.is_empty() {
        None
    } else {
        Some(c)
    }
}

fn strip_jsdoc(text: &str) -> String {
    strip_comment_block(text, "/**")
}

/// Strip a `/* ... */` block comment to its inner text.
/// `open_marker` is the prefix to remove (`"/**"` for JSDoc, `"/*"` for Go/C).
fn strip_comment_block(text: &str, open_marker: &str) -> String {
    text.trim_start_matches(open_marker)
        .trim_end_matches("*/")
        .trim()
        .lines()
        .map(|l| {
            let t = l.trim();
            t.strip_prefix("* ")
                .or_else(|| t.strip_prefix("*"))
                .unwrap_or(t)
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
