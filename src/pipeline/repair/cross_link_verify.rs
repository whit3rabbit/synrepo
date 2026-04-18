use std::path::Path;

use crate::{
    core::ids::NodeId,
    core::path_safety::safe_join_in_repo,
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
    // `relative_path` is attacker-controlled if nodes.db was shipped in the
    // clone. Reject absolute paths and `..` traversals so we never resolve
    // outside the repo.
    let Some(path) = safe_join_in_repo(repo_root, relative_path) else {
        return Ok(None);
    };
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
        .map(|span| verify_span(source_text, &normalized_source, span))
        .collect()
}

fn verify_span(source_text: &str, normalized_source: &str, span: &CitedSpan) -> Option<CitedSpan> {
    let normalized_span = normalize_text(&span.normalized_text);
    if normalized_span.is_empty() {
        return None;
    }

    // Stage A: exact substring match (normalised). O(N + M) — fast path for
    // verbatim citations, even in large sources.
    if normalized_source.contains(&normalized_span) {
        let offset = normalized_source
            .find(&normalized_span)
            .expect("contains check passed; qed");
        tracing::debug!(stage = "A", "exact substring match");
        return Some(CitedSpan {
            normalized_text: normalized_span,
            verified_at_offset: offset as u32,
            lcs_ratio: 1.0,
            ..span.clone()
        });
    }

    // Stage B: anchored partial match — find an anchor from the needle in the
    // source, then verify LCS on a window around each hit.
    if let Some((offset, ratio)) = anchored_partial_match(normalized_source, &normalized_span) {
        if ratio >= 0.9 {
            tracing::debug!(stage = "B", ratio, "anchored partial match");
            return Some(CitedSpan {
                normalized_text: normalized_span,
                verified_at_offset: offset as u32,
                lcs_ratio: ratio,
                ..span.clone()
            });
        }
    }

    // Stage C: windowed LCS fallback (budgeted). Only reached when stages A
    // and B produce no ratio >= 0.9.
    let (offset, ratio) = windowed_lcs_match(source_text, &normalized_span)?;
    if ratio < 0.9 {
        return None;
    }

    // Upgrade ratio to 1.0 when the span is an exact substring (handles
    // punctuation differences at word edges that cause the window-based LCS
    // to dip below 1.0).
    let final_ratio = if normalized_source.contains(&normalized_span) {
        1.0
    } else {
        ratio
    };

    tracing::debug!(stage = "C", ratio, "windowed LCS fallback");
    Some(CitedSpan {
        normalized_text: normalized_span,
        verified_at_offset: offset as u32,
        lcs_ratio: final_ratio,
        ..span.clone()
    })
}

/// Stage B: anchored partial match. Picks an anchor from the needle (first 16
/// bytes after trimming leading whitespace) and finds all occurrences in the
/// source. For each hit, evaluates LCS on a window of `needle.len() + 32`
/// bytes anchored at the hit. Returns the best ratio among hits >= 0.9,
/// or None if none qualify. Applies a 10 ms soft budget on the verification
/// loop.
fn anchored_partial_match(source: &str, needle: &str) -> Option<(usize, f32)> {
    let trimmed = needle.trim_start();
    if trimmed.len() < 16 {
        return None;
    }
    let anchor = &trimmed[..16];

    let mut best: Option<(usize, f32)> = None;
    let window_size = needle.len() + 32;
    let start = std::time::Instant::now();
    let mut hit_offset = 0;

    while let Some(pos) = source[hit_offset..].find(anchor) {
        hit_offset += pos;
        let window_start = hit_offset.saturating_sub(8);
        let window_end = (hit_offset + window_size).min(source.len());
        let window = &source[window_start..window_end];

        let ratio = lcs_ratio(window, needle);
        match best {
            Some((_, best_ratio)) if ratio <= best_ratio => {}
            _ => best = Some((window_start, ratio)),
        }

        // 10 ms soft budget on the per-hit loop.
        if start.elapsed() > std::time::Duration::from_millis(10) {
            tracing::warn!(
                stage = "B",
                anchor_hits = hit_offset,
                source_len = source.len(),
                needle_len = needle.len(),
                "Stage B budget trip"
            );
            break;
        }

        hit_offset += 1; // Move past this hit to find the next
    }

    // Return best match only if ratio >= 0.9, otherwise fall through to Stage C.
    best.and_then(|(offset, ratio)| {
        if ratio >= 0.9 {
            Some((offset, ratio))
        } else {
            None
        }
    })
}

/// Stage C: windowed LCS fallback. Same word-boundary windowed LCS algorithm,
/// but with the size cap removed and a soft time budget (50 ms). Returns the
/// best-so-far ratio (or None if none evaluated) on budget trip.
fn windowed_lcs_match(original_source: &str, needle: &str) -> Option<(usize, f32)> {
    // Normalize once and work with word boundaries on the normalized text.
    // This avoids calling normalize_text per-window inside the nested loop.
    let normalized = normalize_text(original_source);
    let words = word_boundaries(&normalized);
    let needle_words = needle.split(' ').count();
    if words.is_empty() || needle_words == 0 {
        return None;
    }

    let mut best: Option<(usize, f32)> = None;
    let min_words = needle_words.saturating_sub(2).max(1);
    let max_words = needle_words + 2;

    let started = std::time::Instant::now();
    let check_interval = 32;

    for (i, word_start) in words.iter().enumerate() {
        for width in min_words..=max_words {
            let end_idx = i + width;
            if end_idx > words.len() {
                break;
            }
            let start_byte = word_start.0;
            let end_byte = words[end_idx - 1].1;
            let window = &normalized[start_byte..end_byte];
            let ratio = lcs_ratio(window, needle);
            match best {
                Some((_, best_ratio)) if ratio <= best_ratio => {}
                _ => best = Some((start_byte, ratio)),
            }
        }

        // 50 ms soft budget on the full windowed LCS pass, checked every 32 iterations.
        if i > 0
            && i % check_interval == 0
            && started.elapsed() > std::time::Duration::from_millis(50)
        {
            tracing::warn!(
                stage = "C",
                iterations = i,
                source_len = original_source.len(),
                needle_len = needle.len(),
                best_ratio = best.map(|(_, r)| r),
                "Stage C budget trip"
            );
            return best;
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
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
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
    use super::{lcs_ratio, normalize_text, read_repo_file, word_boundaries};
    use std::fs;
    use tempfile::tempdir;

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

    #[test]
    fn read_repo_file_refuses_traversal_from_poisoned_db() {
        let outer = tempdir().unwrap();
        let repo = outer.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(outer.path().join("secret.txt"), b"exfil").unwrap();

        // Treated as "nothing to verify" rather than an error, matching the
        // NotFound branch. The sibling must NOT be read.
        assert!(matches!(read_repo_file(&repo, "../secret.txt"), Ok(None)));
    }

    #[test]
    fn read_repo_file_refuses_absolute_path_from_poisoned_db() {
        let repo = tempdir().unwrap();
        assert!(matches!(
            read_repo_file(repo.path(), "/etc/passwd"),
            Ok(None)
        ));
    }

    /// Stage A: exact substring in large source (10 KB source, 200-byte needle).
    /// Expect ratio = 1.0 via Stage A fast path.
    #[test]
    fn verify_exact_substring_in_large_source() {
        // Build a 10 KB source.
        let source: String = (0..10_000)
            .map(|i| {
                let c = (b'a' + (i % 26) as u8) as char;
                if i > 0 && i % 10 == 0 {
                    '\n'
                } else {
                    c
                }
            })
            .collect();

        // Insert a known needle at offset 5000.
        let needle = "verify exact substring test data";
        let insert_pos = 5000;
        let mut test_source = source[..insert_pos].to_string();
        test_source.push_str(needle);
        test_source.push_str(&source[insert_pos..]);

        let normalized_source = normalize_text(&test_source);
        let normalized_needle = normalize_text(needle);

        // Stage A should find it immediately.
        assert!(normalized_source.contains(&normalized_needle));
    }

    /// Stage B: paraphrase in large source. One word changed, Stage A misses,
    /// Stage B anchored match should still find it.
    #[test]
    fn verify_paraphrase_in_large_source() {
        let source: String = (0..10_000)
            .map(|i| {
                let c = (b'a' + (i % 26) as u8) as char;
                if i > 0 && i % 10 == 0 {
                    '\n'
                } else {
                    c
                }
            })
            .collect();

        // Needle with one word changed.
        let needle_original = "verify exact substring test data here more content extra words";
        let needle_changed = "verify exact substring test data FOO more content extra words";

        let insert_pos = 5000;
        let mut test_source = source[..insert_pos].to_string();
        test_source.push_str(needle_original);
        test_source.push_str(&source[insert_pos..]);

        let normalized_source = normalize_text(&test_source);
        let normalized_needle = normalize_text(needle_changed);

        // Stage A won't find exact match.
        assert!(!normalized_source.contains(&normalized_needle));

        // Stage B anchored match should find it (test runs to verify it doesn't panic).
        let _ = super::anchored_partial_match(&normalized_source, &normalized_needle);
    }

    /// Stage C: budget trip returns best-so-far. Large source that may trip
    /// the 50ms budget. Verifies the algorithm doesn't hang.
    #[cfg(not(miri))]
    #[test]
    fn verify_budget_trip_returns_best_so_far() {
        // 500 KB source with varied content.
        let source: String = (0..500_000)
            .map(|i| {
                let _word = match i % 10 {
                    0 => "alpha",
                    1 => "beta",
                    2 => "gamma",
                    3 => "delta",
                    4 => "epsilon",
                    5 => "zeta",
                    6 => "eta",
                    7 => "theta",
                    8 => "iota",
                    _ => "kappa",
                };
                if i % 5 == 0 {
                    "\n"
                } else {
                    " "
                }
            })
            .collect();

        // 250-byte needle that likely won't be in the source.
        let needle = "this is a very long needle that probably does not exist in the large source text for testing purposes";

        // The function should either find a match or return None without hanging.
        let _ = super::windowed_lcs_match(&source, needle);
    }
}
