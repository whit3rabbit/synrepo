use super::{node_text, Language};

/// Extract the doc comment for a definition node by language-specific strategy.
///
/// Rust: walk preceding siblings collecting contiguous `///` line comments.
/// Python: read the first statement of the body if it is a string literal.
/// TypeScript/TSX and JavaScript: find the nearest preceding `/**` block comment.
/// Java/Kotlin/PHP: find the nearest preceding `/**` block comment.
/// C#/Swift/Dart: find nearest preceding `///` or `/**` doc comment.
/// C/C++: find nearest preceding Doxygen-style `///`, `//!`, `/**`, or `/*!`.
/// Ruby: collect contiguous preceding `#` comments.
pub(super) fn extract_doc_comment(
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
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            preceding_doc_comment(item_node, source, &[], &["/**"], &["decorator"])
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
        Language::Java | Language::Kotlin | Language::Php => {
            preceding_doc_comment(item_node, source, &[], &["/**"], &[])
        }
        Language::CSharp | Language::Swift | Language::Dart => {
            preceding_doc_comment(item_node, source, &["///"], &["/**"], &[])
        }
        Language::C | Language::Cpp => {
            preceding_doc_comment(item_node, source, &["///", "//!"], &["/**", "/*!"], &[])
        }
        Language::Ruby => preceding_doc_comment(item_node, source, &["#"], &[], &[]),
    }
}

/// Extract the declaration signature for a definition node.
///
/// Returns the text from the start of the node to the first body delimiter
/// (`{` or `;` for Rust, `:` at depth-0 for Python, `{` for TypeScript/TSX),
/// with internal whitespace collapsed to a single space and capped at 200 chars.
pub(super) fn extract_signature(
    item_node: tree_sitter::Node,
    source: &[u8],
    language: Language,
) -> Option<String> {
    let offset = item_node.start_byte();
    let text = node_text(item_node, source);
    let sig = match language {
        Language::Rust | Language::Go => {
            let end_byte = if let Some(body) = item_node.child_by_field_name("body") {
                body.start_byte()
            } else if let Some(block) = item_node.child_by_field_name("block") {
                block.start_byte()
            } else {
                offset + text.find(['{', ';']).unwrap_or(text.len())
            };
            let end = end_byte.saturating_sub(offset).min(text.len());
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
            if matches!(
                item_node.kind(),
                "variable_declaration" | "lexical_declaration"
            ) {
                if let Some(p) = text.find('=') {
                    return Some(collapse_ws(&text[..p]));
                }
            }
            let end_byte = if let Some(body) = item_node.child_by_field_name("body") {
                body.start_byte()
            } else {
                offset + text.find('{').unwrap_or(text.len())
            };
            let end = end_byte.saturating_sub(offset).min(text.len());
            collapse_ws(&text[..end])
        }
        Language::JavaScript
        | Language::Java
        | Language::Kotlin
        | Language::CSharp
        | Language::Php
        | Language::Ruby
        | Language::Swift
        | Language::C
        | Language::Cpp
        | Language::Dart => collapse_ws(&text[..text.find('{').unwrap_or(text.len())]),
    };
    if sig.is_empty() {
        None
    } else {
        Some(sig)
    }
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
    let c = if r.starts_with("\"\"\"") && r.ends_with("\"\"\"") && r.len() >= 6 {
        r.strip_prefix("\"\"\"")
            .and_then(|s| s.strip_suffix("\"\"\""))
            .unwrap()
            .trim()
            .to_string()
    } else if r.starts_with("'''") && r.ends_with("'''") && r.len() >= 6 {
        r.strip_prefix("'''")
            .and_then(|s| s.strip_suffix("'''"))
            .unwrap()
            .trim()
            .to_string()
    } else if r.starts_with('"') && r.ends_with('"') && r.len() >= 2 {
        r.strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap()
            .to_string()
    } else if r.starts_with('\'') && r.ends_with('\'') && r.len() >= 2 {
        r.strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .unwrap()
            .to_string()
    } else {
        return None;
    };
    if c.is_empty() {
        None
    } else {
        Some(c)
    }
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

fn preceding_doc_comment(
    item_node: tree_sitter::Node,
    source: &[u8],
    line_prefixes: &[&str],
    block_prefixes: &[&str],
    skip_kinds: &[&str],
) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let anchor = doc_comment_anchor(item_node);
    let mut prev = anchor.prev_named_sibling();
    while let Some(node) = prev {
        if skip_kinds.contains(&node.kind()) {
            prev = node.prev_named_sibling();
            continue;
        }
        if !is_comment_node(node.kind()) {
            break;
        }
        let text = node_text(node, source);
        if let Some(stripped) = strip_line_comment(&text, line_prefixes) {
            lines.push(stripped);
            prev = node.prev_named_sibling();
            continue;
        }
        if lines.is_empty() {
            if let Some(stripped) = strip_block_doc_comment(&text, block_prefixes) {
                return (!stripped.is_empty()).then_some(stripped);
            }
        }
        break;
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    let joined = lines.join("\n");
    (!joined.is_empty()).then_some(joined)
}

fn strip_line_comment(text: &str, prefixes: &[&str]) -> Option<String> {
    let trimmed = text.trim_start();
    prefixes
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
        .map(|rest| rest.trim().to_string())
}

fn strip_block_doc_comment(text: &str, prefixes: &[&str]) -> Option<String> {
    let trimmed = text.trim_start();
    prefixes
        .iter()
        .find(|prefix| trimmed.starts_with(**prefix))
        .map(|prefix| strip_comment_block(trimmed, prefix))
}

fn is_comment_node(kind: &str) -> bool {
    matches!(
        kind,
        "comment" | "line_comment" | "block_comment" | "multiline_comment"
    )
}

fn doc_comment_anchor(item_node: tree_sitter::Node) -> tree_sitter::Node {
    let mut anchor = item_node;
    while let Some(parent) = anchor.parent() {
        if is_doc_comment_wrapper(parent.kind(), anchor.kind()) {
            anchor = parent;
        } else {
            break;
        }
    }
    anchor
}

fn is_doc_comment_wrapper(parent_kind: &str, child_kind: &str) -> bool {
    matches!(
        parent_kind,
        "export_statement" | "lexical_declaration" | "variable_declaration" | "declaration"
    ) || parent_kind == "variable_declarator"
        || (parent_kind == "function_declaration" && child_kind == "function_signature")
}
