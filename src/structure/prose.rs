//! Markdown concept extraction for the structural pipeline.
//!
//! Produces `ConceptNode` records from human-authored markdown files in
//! configured concept directories. Only the title, aliases, and a short
//! summary are extracted here — semantic tagging and cross-linking are
//! overlay concerns and do not belong in the structural pipeline.

use crate::core::ids::ConceptNodeId;
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::structure::graph::{ConceptNode, Epistemic};
use time::OffsetDateTime;

/// Extract a `ConceptNode` from human-authored markdown content.
///
/// The concept title is taken from the frontmatter `title:` key if present,
/// otherwise from the first `# ` heading in the document body. Aliases come
/// from the frontmatter `aliases:` list. The summary is the first non-empty
/// paragraph after any heading.
///
/// `path` must be the repo-relative path (used for the node's `path` field
/// and for deriving the stable `ConceptNodeId`).
pub fn extract_concept(
    path: &str,
    content: &[u8],
    revision: &str,
) -> crate::Result<Option<ConceptNode>> {
    let text = match std::str::from_utf8(content) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let (frontmatter, body) = split_frontmatter(text);

    let title = frontmatter
        .as_deref()
        .and_then(|fm| extract_frontmatter_scalar(fm, "title"))
        .or_else(|| extract_h1_title(body))
        .unwrap_or_else(|| path_basename(path).to_string());

    let aliases = frontmatter
        .as_deref()
        .map(|fm| extract_frontmatter_list(fm, "aliases"))
        .unwrap_or_default();

    let summary = extract_first_paragraph(body);

    let id = derive_concept_id(path);
    let content_hash = hex::encode(blake3::hash(content).as_bytes());

    let provenance = Provenance {
        created_at: OffsetDateTime::now_utc(),
        source_revision: revision.to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "parse_prose".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None, // concept nodes reference markdown source by path, not graph FileNodeId
            path: path.to_string(),
            content_hash,
        }],
    };

    Ok(Some(ConceptNode {
        id,
        path: path.to_string(),
        title,
        aliases,
        summary,
        epistemic: Epistemic::HumanDeclared,
        provenance,
    }))
}

/// Derive a stable `ConceptNodeId` from the repo-relative path.
///
/// Path-derived IDs stay stable across content edits: when the author
/// updates the document, the concept node is updated in-place rather
/// than replaced with a new identity.
pub fn derive_concept_id(path: &str) -> ConceptNodeId {
    let hash = blake3::hash(path.as_bytes());
    let bytes = hash.as_bytes();
    let id = u64::from_le_bytes(bytes[0..8].try_into().expect("blake3 output >= 8 bytes"));
    ConceptNodeId(id)
}

/// Split a markdown document into optional frontmatter and body text.
///
/// Frontmatter is recognized as a block of text between the first `---`
/// line and the next `---` line, when those lines appear at the very
/// start of the document (before any non-empty content).
fn split_frontmatter(text: &str) -> (Option<String>, &str) {
    let trimmed = text.trim_start_matches('\n');
    if !trimmed.starts_with("---") {
        return (None, text);
    }

    // The opening `---` must be followed by a newline.
    let after_open = match trimmed.strip_prefix("---") {
        Some(rest) if rest.starts_with('\n') || rest.starts_with("\r\n") => {
            rest.trim_start_matches('\n').trim_start_matches('\r')
        }
        _ => return (None, text),
    };

    // Find the closing `---`.
    let closing_marker = "\n---";
    if let Some(pos) = after_open.find(closing_marker) {
        let fm = after_open[..pos].to_string();
        let body = after_open[pos + closing_marker.len()..].trim_start_matches('\n');
        (Some(fm), body)
    } else {
        (None, text)
    }
}

/// Extract a single-value key from a YAML-light frontmatter block.
///
/// Handles `key: value` and `key: "quoted value"` forms.
fn extract_frontmatter_scalar(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(rest) = rest.strip_prefix(':') {
                let value = rest.trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Extract a list value from a YAML-light frontmatter block.
///
/// Supports both inline arrays (`aliases: [foo, bar]`) and block lists:
/// ```yaml
/// aliases:
///   - foo
///   - bar
/// ```
fn extract_frontmatter_list(frontmatter: &str, key: &str) -> Vec<String> {
    let lines: Vec<&str> = frontmatter.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            if let Some(rest) = rest.strip_prefix(':') {
                let rest = rest.trim();

                // Inline array: [foo, bar]
                if rest.starts_with('[') {
                    let inner = rest.trim_matches(|c| c == '[' || c == ']');
                    return inner
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }

                // Block list: subsequent lines starting with "  - "
                if rest.is_empty() {
                    let mut result = Vec::new();
                    for next_line in lines.iter().skip(i + 1) {
                        let nl = next_line.trim();
                        if let Some(item) = nl.strip_prefix('-') {
                            let item = item.trim().trim_matches('"').trim_matches('\'');
                            if !item.is_empty() {
                                result.push(item.to_string());
                            }
                        } else if !nl.is_empty() {
                            break;
                        }
                    }
                    return result;
                }
            }
        }
    }

    Vec::new()
}

/// Extract the title from the first `# ` heading in the body.
fn extract_h1_title(body: &str) -> Option<String> {
    for line in body.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

/// Extract the first non-empty paragraph from the body (after skipping headings).
fn extract_first_paragraph(body: &str) -> Option<String> {
    let mut in_para = false;
    let mut para_lines: Vec<&str> = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();

        // Skip headings and blank lines before first paragraph.
        if trimmed.starts_with('#') {
            if in_para {
                break;
            }
            continue;
        }

        if trimmed.is_empty() {
            if in_para {
                break;
            }
            continue;
        }

        in_para = true;
        para_lines.push(trimmed);
    }

    if para_lines.is_empty() {
        None
    } else {
        Some(para_lines.join(" "))
    }
}

/// Return the filename stem from a repo-relative path, for use as a fallback title.
fn path_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_concept_reads_h1_title_and_first_paragraph() {
        let md = b"# Graph Storage\n\nWhy the graph stays observed-only.\n";
        let node = extract_concept("docs/adr/0001-graph.md", md, "abc123")
            .unwrap()
            .unwrap();

        assert_eq!(node.title, "Graph Storage");
        assert_eq!(node.summary.as_deref(), Some("Why the graph stays observed-only."));
        assert_eq!(node.path, "docs/adr/0001-graph.md");
        assert!(node.aliases.is_empty());
    }

    #[test]
    fn extract_concept_prefers_frontmatter_title_over_h1() {
        let md = b"---\ntitle: Override Title\n---\n# Body Title\n\nSummary text.\n";
        let node = extract_concept("docs/adr/0002.md", md, "rev")
            .unwrap()
            .unwrap();

        assert_eq!(node.title, "Override Title");
    }

    #[test]
    fn extract_concept_parses_inline_alias_array() {
        let md = b"---\ntitle: My Concept\naliases: [foo, bar, baz]\n---\nContent here.\n";
        let node = extract_concept("docs/concepts/my.md", md, "rev")
            .unwrap()
            .unwrap();

        assert_eq!(node.title, "My Concept");
        assert_eq!(node.aliases, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn extract_concept_parses_block_alias_list() {
        let md = b"---\ntitle: My Concept\naliases:\n  - foo\n  - bar\n---\nContent.\n";
        let node = extract_concept("docs/concepts/my.md", md, "rev")
            .unwrap()
            .unwrap();

        assert_eq!(node.aliases, vec!["foo", "bar"]);
    }

    #[test]
    fn extract_concept_falls_back_to_filename_when_no_title() {
        let md = b"No heading here, just prose.\n";
        let node = extract_concept("docs/adr/0003-decision.md", md, "rev")
            .unwrap()
            .unwrap();

        assert_eq!(node.title, "0003-decision.md");
    }

    #[test]
    fn derive_concept_id_is_path_stable() {
        let id1 = derive_concept_id("docs/adr/0001.md");
        let id2 = derive_concept_id("docs/adr/0001.md");
        assert_eq!(id1, id2);
    }

    #[test]
    fn derive_concept_id_differs_by_path() {
        let id1 = derive_concept_id("docs/adr/0001.md");
        let id2 = derive_concept_id("docs/adr/0002.md");
        assert_ne!(id1, id2);
    }
}
