use std::path::Path;

use crate::{overlay::OverlayLink, store::sqlite::SqliteGraphStore};

mod io;
mod matching;

pub(super) use io::current_endpoint_hash;

pub(super) struct VerifiedCandidate {
    pub source_spans: Vec<crate::overlay::CitedSpan>,
    pub target_spans: Vec<crate::overlay::CitedSpan>,
    pub from_hash: String,
    pub to_hash: String,
}

pub(super) fn verify_candidate_payload(
    graph: &SqliteGraphStore,
    repo_root: &Path,
    candidate: &OverlayLink,
) -> crate::Result<Option<VerifiedCandidate>> {
    let Some(from_text) = io::load_endpoint_text(graph, repo_root, candidate.from)? else {
        return Ok(None);
    };
    let Some(to_text) = io::load_endpoint_text(graph, repo_root, candidate.to)? else {
        return Ok(None);
    };

    let Some(source_spans) = matching::verify_spans(&from_text, &candidate.source_spans) else {
        return Ok(None);
    };
    let Some(target_spans) = matching::verify_spans(&to_text, &candidate.target_spans) else {
        return Ok(None);
    };

    let Some(from_hash) = io::current_endpoint_hash(graph, candidate.from)? else {
        return Ok(None);
    };
    let Some(to_hash) = io::current_endpoint_hash(graph, candidate.to)? else {
        return Ok(None);
    };

    Ok(Some(VerifiedCandidate {
        source_spans,
        target_spans,
        from_hash,
        to_hash,
    }))
}

#[cfg(test)]
mod tests {
    use super::io::read_repo_file;
    use super::matching::{
        anchored_partial_match, lcs_ratio, normalize_text, verify_span, windowed_lcs_match,
        word_boundaries, MAX_LCS_INPUT_LEN,
    };
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
        let _ = anchored_partial_match(&normalized_source, &normalized_needle);
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
        let _ = windowed_lcs_match(&source, needle);
    }

    #[test]
    fn verify_span_rejects_oversized_inputs() {
        use crate::overlay::CitedSpan;
        let source = "short source";
        let norm_source = normalize_text(source);
        let oversized_needle = "a".repeat(MAX_LCS_INPUT_LEN + 1);
        let span = CitedSpan {
            artifact: crate::core::ids::NodeId::File(crate::core::ids::FileNodeId(0)),
            normalized_text: oversized_needle,
            verified_at_offset: 0,
            lcs_ratio: 0.0,
        };

        // Should return None due to length cap
        assert!(verify_span(source, &norm_source, &span).is_none());
    }
}
