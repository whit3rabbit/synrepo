use std::path::Path;

use crate::{
    core::ids::NodeId,
    overlay::{CitedSpan, OverlayLink},
    store::sqlite::SqliteGraphStore,
    structure::graph::GraphStore,
};

pub(super) struct VerifiedCandidate {
    pub source_spans: Vec<CitedSpan>,
    pub target_spans: Vec<CitedSpan>,
    pub from_hash: String,
    pub to_hash: String,
}

pub(super) fn verify_candidate_payload(
    graph: &SqliteGraphStore,
    repo_root: &Path,
    candidate: &OverlayLink,
) -> crate::Result<Option<VerifiedCandidate>> {
    let Some(from_text) = load_endpoint_text(graph, repo_root, candidate.from)? else {
        return Ok(None);
    };
    let Some(to_text) = load_endpoint_text(graph, repo_root, candidate.to)? else {
        return Ok(None);
    };

    let Some(source_spans) = verify_spans(&from_text, &candidate.source_spans) else {
        return Ok(None);
    };
    let Some(target_spans) = verify_spans(&to_text, &candidate.target_spans) else {
        return Ok(None);
    };

    let Some(from_hash) = current_endpoint_hash(graph, candidate.from)? else {
        return Ok(None);
    };
    let Some(to_hash) = current_endpoint_hash(graph, candidate.to)? else {
        return Ok(None);
    };

    Ok(Some(VerifiedCandidate {
        source_spans,
        target_spans,
        from_hash,
        to_hash,
    }))
}

pub(super) fn current_endpoint_hash(
    graph: &SqliteGraphStore,
    node: NodeId,
) -> crate::Result<Option<String>> {
    match node {
        NodeId::File(file_id) => Ok(graph.get_file(file_id)?.map(|file| file.content_hash)),
        NodeId::Symbol(symbol_id) => {
            let Some(symbol) = graph.get_symbol(symbol_id)? else {
                return Ok(None);
            };
            Ok(graph
                .get_file(symbol.file_id)?
                .map(|file| file.content_hash))
        }
        NodeId::Concept(concept_id) => {
            let Some(concept) = graph.get_concept(concept_id)? else {
                return Ok(None);
            };
            if let Some(file) = graph.file_by_path(&concept.path)? {
                return Ok(Some(file.content_hash));
            }
            Ok(concept
                .provenance
                .source_artifacts
                .first()
                .map(|source| source.content_hash.clone()))
        }
    }
}

fn load_endpoint_text(
    graph: &SqliteGraphStore,
    repo_root: &Path,
    node: NodeId,
) -> crate::Result<Option<String>> {
    match node {
        NodeId::File(file_id) => {
            let Some(file) = graph.get_file(file_id)? else {
                return Ok(None);
            };
            read_repo_file(repo_root, &file.path)
        }
        NodeId::Symbol(symbol_id) => {
            let Some(symbol) = graph.get_symbol(symbol_id)? else {
                return Ok(None);
            };
            let Some(file) = graph.get_file(symbol.file_id)? else {
                return Ok(None);
            };
            let Some(source) = read_repo_file(repo_root, &file.path)? else {
                return Ok(None);
            };
            let start = symbol.body_byte_range.0 as usize;
            let end = symbol.body_byte_range.1 as usize;
            if let Some(slice) = source.get(start..end) {
                return Ok(Some(slice.to_string()));
            }
            Ok(Some(source))
        }
        NodeId::Concept(concept_id) => {
            let Some(concept) = graph.get_concept(concept_id)? else {
                return Ok(None);
            };
            read_repo_file(repo_root, &concept.path)
        }
    }
}

fn read_repo_file(repo_root: &Path, relative_path: &str) -> crate::Result<Option<String>> {
    let path = repo_root.join(relative_path);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn verify_spans(source_text: &str, spans: &[CitedSpan]) -> Option<Vec<CitedSpan>> {
    let normalized_source = normalize_text(source_text);
    spans
        .iter()
        .map(|span| verify_span(&normalized_source, span))
        .collect()
}

fn verify_span(normalized_source: &str, span: &CitedSpan) -> Option<CitedSpan> {
    let normalized_span = normalize_text(&span.normalized_text);
    if normalized_span.is_empty() {
        return None;
    }

    if let Some(offset) = normalized_source.find(&normalized_span) {
        return Some(CitedSpan {
            normalized_text: normalized_span,
            verified_at_offset: offset as u32,
            lcs_ratio: 1.0,
            ..span.clone()
        });
    }

    let (offset, ratio) = best_fuzzy_match(normalized_source, &normalized_span)?;
    if ratio < 0.9 {
        return None;
    }

    Some(CitedSpan {
        normalized_text: normalized_span,
        verified_at_offset: offset as u32,
        lcs_ratio: ratio,
        ..span.clone()
    })
}

fn best_fuzzy_match(source: &str, needle: &str) -> Option<(usize, f32)> {
    if source.len() > 4096 || needle.len() > 256 {
        return None;
    }

    let words = word_boundaries(source);
    let needle_words = needle.split(' ').count();
    if words.is_empty() || needle_words == 0 {
        return None;
    }

    let mut best: Option<(usize, f32)> = None;
    let min_words = needle_words.saturating_sub(2).max(1);
    let max_words = needle_words + 2;

    for start in 0..words.len() {
        for width in min_words..=max_words {
            let end = start + width;
            if end > words.len() {
                break;
            }
            let start_byte = words[start].0;
            let end_byte = words[end - 1].1;
            let window = &source[start_byte..end_byte];
            let ratio = lcs_ratio(window, needle);
            match best {
                Some((_, best_ratio)) if ratio <= best_ratio => {}
                _ => best = Some((start_byte, ratio)),
            }
        }
    }

    best
}

fn lcs_ratio(left: &str, right: &str) -> f32 {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let mut prev = vec![0usize; right.len() + 1];
    let mut curr = vec![0usize; right.len() + 1];
    for &left_byte in left {
        for (index, &right_byte) in right.iter().enumerate() {
            curr[index + 1] = if left_byte == right_byte {
                prev[index] + 1
            } else {
                prev[index + 1].max(curr[index])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }

    prev[right.len()] as f32 / left.len().max(right.len()) as f32
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn word_boundaries(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        while index < bytes.len() && bytes[index] == b' ' {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && bytes[index] != b' ' {
            index += 1;
        }
        if start < index {
            out.push((start, index));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{lcs_ratio, normalize_text, word_boundaries};

    #[test]
    fn normalize_text_collapses_whitespace() {
        assert_eq!(normalize_text(" a\tb\nc "), "a b c");
    }

    #[test]
    fn lcs_ratio_is_perfect_for_exact_match() {
        assert_eq!(lcs_ratio("authenticate", "authenticate"), 1.0);
    }

    #[test]
    fn word_boundaries_track_byte_ranges() {
        assert_eq!(word_boundaries("alpha beta"), vec![(0, 5), (6, 10)]);
    }
}
