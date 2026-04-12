//! Markdown concept extraction for the structural pipeline.
//!
//! Produces `ConceptNode` records from human-authored markdown files in
//! configured concept directories. Only the title, aliases, summary, status,
//! and decision body are extracted — semantic tagging and cross-linking are
//! overlay concerns and do not belong in the structural pipeline.

use crate::core::ids::ConceptNodeId;
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::structure::graph::{ConceptNode, Epistemic};
use time::OffsetDateTime;

/// Extract a `ConceptNode` and the governs paths declared in its frontmatter.
///
/// The second tuple element is the raw `governs:` path list from frontmatter;
/// the caller resolves these to `FileNodeId`s and emits `Governs` edges.
/// Returns `None` when the content is not valid UTF-8.
///
/// `path` must be the repo-relative path used for `ConceptNodeId` derivation.
pub fn extract_concept(
    path: &str,
    content: &[u8],
    revision: &str,
) -> crate::Result<Option<(ConceptNode, Vec<String>)>> {
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
    let status = frontmatter
        .as_deref()
        .and_then(|fm| extract_frontmatter_scalar(fm, "status"));
    let decision_body = extract_decision_body(body);
    let governs_paths = frontmatter
        .as_deref()
        .map(extract_governs_paths)
        .unwrap_or_default();

    let id = derive_concept_id(path);
    let content_hash = hex::encode(blake3::hash(content).as_bytes());
    let provenance = Provenance {
        created_at: OffsetDateTime::now_utc(),
        source_revision: revision.to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "parse_prose".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash,
        }],
    };

    Ok(Some((
        ConceptNode {
            id,
            path: path.to_string(),
            title,
            aliases,
            summary,
            status,
            decision_body,
            epistemic: Epistemic::HumanDeclared,
            provenance,
        },
        governs_paths,
    )))
}

/// Derive a stable `ConceptNodeId` from the repo-relative path.
pub fn derive_concept_id(path: &str) -> ConceptNodeId {
    let hash = blake3::hash(path.as_bytes());
    let bytes = hash.as_bytes();
    let id = u64::from_le_bytes(bytes[0..8].try_into().expect("blake3 output >= 8 bytes"));
    ConceptNodeId(id)
}

/// Extract the relative file paths declared in the `governs:` frontmatter key.
///
/// Supports both inline (`governs: [src/foo.rs]`) and block-list YAML format.
/// Returns an empty vec when the key is absent.
pub fn extract_governs_paths(frontmatter: &str) -> Vec<String> {
    extract_frontmatter_list(frontmatter, "governs")
}

/// Extract the decision body from markdown body text.
///
/// Tries `## Decision`, then `## Context`, then returns the full body.
fn extract_decision_body(body: &str) -> Option<String> {
    for heading in &["## Decision", "## Context"] {
        if let Some(text) = extract_section(body, heading) {
            return Some(text);
        }
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extract text between a `## Heading` line and the next `## ` heading.
fn extract_section(body: &str, heading: &str) -> Option<String> {
    let needle = format!("\n{heading}\n");
    let start = if let Some(p) = body.find(&needle) {
        p + needle.len()
    } else if body.starts_with(heading) && body[heading.len()..].starts_with('\n') {
        heading.len() + 1
    } else {
        return None;
    };
    let after = &body[start..];
    let end = after.find("\n## ").unwrap_or(after.len());
    let text = after[..end].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn split_frontmatter(text: &str) -> (Option<String>, &str) {
    let trimmed = text.trim_start_matches('\n');
    if !trimmed.starts_with("---") {
        return (None, text);
    }
    let after_open = match trimmed.strip_prefix("---") {
        Some(rest) if rest.starts_with('\n') || rest.starts_with("\r\n") => {
            rest.trim_start_matches('\n').trim_start_matches('\r')
        }
        _ => return (None, text),
    };
    let closing_marker = "\n---";
    if let Some(pos) = after_open.find(closing_marker) {
        let fm = after_open[..pos].to_string();
        let body = after_open[pos + closing_marker.len()..].trim_start_matches('\n');
        (Some(fm), body)
    } else {
        (None, text)
    }
}

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

fn extract_frontmatter_list(frontmatter: &str, key: &str) -> Vec<String> {
    let lines: Vec<&str> = frontmatter.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            if let Some(rest) = rest.strip_prefix(':') {
                let rest = rest.trim();
                if rest.starts_with('[') {
                    let inner = rest.trim_matches(|c| c == '[' || c == ']');
                    return inner
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
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

fn extract_h1_title(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| {
            line.strip_prefix("# ")
                .map(str::trim)
                .filter(|t| !t.is_empty())
        })
        .map(str::to_string)
}

fn extract_first_paragraph(body: &str) -> Option<String> {
    let mut in_para = false;
    let mut para_lines: Vec<&str> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
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

fn path_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn concept(md: &[u8]) -> (ConceptNode, Vec<String>) {
        extract_concept("docs/adr/test.md", md, "rev")
            .unwrap()
            .unwrap()
    }

    #[test]
    fn extract_concept_title_and_summary() {
        let (node, governs) = concept(b"# Graph Storage\n\nWhy observed-only.\n");
        assert_eq!(node.title, "Graph Storage");
        assert_eq!(node.summary.as_deref(), Some("Why observed-only."));
        assert!(governs.is_empty());
    }

    #[test]
    fn extract_concept_frontmatter_title_overrides_h1() {
        let (node, _) = concept(b"---\ntitle: Override\n---\n# Body\n\nText.\n");
        assert_eq!(node.title, "Override");
    }

    #[test]
    fn extract_concept_aliases_inline_and_block() {
        let (node, _) = concept(b"---\ntitle: A\naliases: [x, y]\n---\n");
        assert_eq!(node.aliases, vec!["x", "y"]);

        let (node2, _) = concept(b"---\ntitle: A\naliases:\n  - p\n  - q\n---\n");
        assert_eq!(node2.aliases, vec!["p", "q"]);
    }

    #[test]
    fn extract_concept_status_and_decision_body() {
        let md =
            b"---\ntitle: Use SQLite\nstatus: Accepted\n---\n\n## Context\n\nBackground.\n\n## Decision\n\nWe use SQLite.\n\n## Consequences\n\nFast.\n";
        let (node, _) = concept(md);
        assert_eq!(node.status.as_deref(), Some("Accepted"));
        assert_eq!(node.decision_body.as_deref(), Some("We use SQLite."));
    }

    #[test]
    fn extract_concept_decision_body_falls_back_to_context() {
        let md = b"---\nstatus: Accepted\n---\n\n## Context\n\nBackground info.\n";
        let (node, _) = concept(md);
        assert_eq!(node.decision_body.as_deref(), Some("Background info."));
    }

    #[test]
    fn extract_governs_paths_formats() {
        // inline array
        let paths = extract_governs_paths("governs: [src/lib.rs, src/store/mod.rs]\n");
        assert_eq!(paths, vec!["src/lib.rs", "src/store/mod.rs"]);
        // block list
        let paths = extract_governs_paths("governs:\n  - src/lib.rs\n  - src/config.rs\n");
        assert_eq!(paths, vec!["src/lib.rs", "src/config.rs"]);
        // absent key -> empty
        assert!(extract_governs_paths("title: ADR\n").is_empty());
        // empty inline list
        assert!(extract_governs_paths("governs: []\n").is_empty());
    }

    #[test]
    fn extract_concept_returns_governs_paths() {
        let (_, governs) = concept(b"---\ntitle: A\ngoverns: [src/lib.rs]\n---\nContent.\n");
        assert_eq!(governs, vec!["src/lib.rs"]);
    }

    #[test]
    fn derive_concept_id_stable_and_distinct() {
        let id1 = derive_concept_id("docs/adr/0001.md");
        assert_eq!(id1, derive_concept_id("docs/adr/0001.md"));
        assert_ne!(id1, derive_concept_id("docs/adr/0002.md"));
    }
}
