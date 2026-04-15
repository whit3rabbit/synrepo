use super::{node_text, Language};

/// Extract the doc comment for a definition node by language-specific strategy.
///
/// Rust: walk preceding siblings collecting contiguous `///` line comments.
/// Python: read the first statement of the body if it is a string literal.
/// TypeScript/TSX: find the nearest preceding `/**` block comment.
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
pub(super) fn extract_signature(
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
        // REVIEW NOTE: the `r.len() < 6` early return guards the slice
        // below. `starts_with` + `ends_with` on the same 3-byte prefix both
        // match when `r == "\"\"\""` (len 3), so without this check the
        // slice `r[3..r.len() - 3]` would underflow and panic. Do not
        // remove.
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
