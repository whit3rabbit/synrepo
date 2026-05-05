//! Symbol-set similarity detection for split/merge rename cases.

use std::collections::{HashMap, HashSet};

use super::IdentityResolution;
use crate::core::ids::FileNodeId;
use crate::structure::graph::{FileNode, GraphStore};

/// Jaccard similarity threshold for split detection (one -> many).
const SPLIT_THRESHOLD: f64 = 0.4;
/// Jaccard similarity threshold for merge detection (many -> one).
const MERGE_THRESHOLD: f64 = 0.5;
/// Jaccard threshold for single-file rename detection.
const RENAME_SYMBOL_THRESHOLD: f64 = 0.8;
/// Minimum gap between the best and second-best rename candidate.
const RENAME_DOMINANCE_MARGIN: f64 = 0.15;
/// Files with at most this many symbols may use sampled content similarity.
const SYMBOL_POOR_MAX_SYMBOLS: usize = 1;
/// Hard cap for sampled-content rename similarity.
const SAMPLE_FILE_SIZE_CAP_BYTES: u64 = 256 * 1024;
/// Bytes sampled from the start, middle, and end of a file.
const SAMPLE_WINDOW_BYTES: usize = 4 * 1024;
/// Shingle width inside each sampled window.
const SHINGLE_BYTES: usize = 32;
/// Sampled-content Jaccard threshold for symbol-poor rename detection.
const SAMPLE_RENAME_THRESHOLD: f64 = 0.72;

/// Build bounded sampled-content shingle hashes for future rename detection.
pub fn sampled_content_hashes(content: &[u8]) -> Vec<u64> {
    if content.len() as u64 > SAMPLE_FILE_SIZE_CAP_BYTES || content.len() < SHINGLE_BYTES {
        return Vec::new();
    }

    let mut hashes = HashSet::new();
    for (start, end) in sample_windows(content.len()) {
        let window = &content[start..end];
        if window.len() < SHINGLE_BYTES {
            continue;
        }
        for shingle in window.windows(SHINGLE_BYTES).step_by(SHINGLE_BYTES) {
            let digest = blake3::hash(shingle);
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&digest.as_bytes()[..8]);
            hashes.insert(u64::from_le_bytes(bytes));
        }
    }

    let mut out = hashes.into_iter().collect::<Vec<_>>();
    out.sort_unstable();
    out
}

fn sample_windows(len: usize) -> Vec<(usize, usize)> {
    if len <= SAMPLE_WINDOW_BYTES {
        return vec![(0, len)];
    }
    let last = len - SAMPLE_WINDOW_BYTES;
    let middle = (len / 2).saturating_sub(SAMPLE_WINDOW_BYTES / 2).min(last);
    let mut windows = vec![
        (0, SAMPLE_WINDOW_BYTES),
        (middle, middle + SAMPLE_WINDOW_BYTES),
    ];
    if last != 0 && last != middle {
        windows.push((last, len));
    }
    windows.sort_unstable();
    windows.dedup();
    windows
}

/// Collect the set of qualified symbol names defined in a file.
pub(super) fn symbol_set_for_file(
    file_id: FileNodeId,
    graph: &dyn GraphStore,
) -> crate::Result<HashSet<String>> {
    use crate::core::ids::NodeId;
    use crate::structure::graph::EdgeKind;

    let edges = graph.outbound(NodeId::File(file_id), Some(EdgeKind::Defines))?;
    let mut names = HashSet::new();
    for edge in &edges {
        if let NodeId::Symbol(sym_id) = edge.to {
            if let Some(sym) = graph.get_symbol(sym_id)? {
                names.insert(sym.qualified_name.clone());
            }
        }
    }
    Ok(names)
}

/// Compute Jaccard similarity between two sets.
fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    intersection / union
}

fn sample_similarity(a: &[u64], b: &[u64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a = a.iter().copied().collect::<HashSet<_>>();
    let b = b.iter().copied().collect::<HashSet<_>>();
    let intersection = a.intersection(&b).count() as f64;
    let union = a.union(&b).count() as f64;
    intersection / union
}

/// Detect a single edited rename using high-overlap evidence.
pub(super) fn detect_rename(
    disappeared: &FileNode,
    new_files: &[FileNode],
    old_symbol_sets: &HashMap<FileNodeId, HashSet<String>>,
    new_symbol_sets: &HashMap<FileNodeId, HashSet<String>>,
) -> crate::Result<Option<IdentityResolution>> {
    let old_symbols = old_symbol_sets
        .get(&disappeared.id)
        .cloned()
        .unwrap_or_default();
    let mut matches = Vec::<(f64, &FileNode)>::new();

    for new_file in new_files
        .iter()
        .filter(|file| file.root_id == disappeared.root_id)
    {
        let new_symbols = new_symbol_sets
            .get(&new_file.id)
            .cloned()
            .unwrap_or_default();
        let symbol_score = jaccard_similarity(&old_symbols, &new_symbols);
        let score = if old_symbols.len() <= SYMBOL_POOR_MAX_SYMBOLS
            || new_symbols.len() <= SYMBOL_POOR_MAX_SYMBOLS
        {
            let weak_symbol_score = if old_symbols.is_empty() || new_symbols.is_empty() {
                0.0
            } else {
                symbol_score
            };
            if disappeared.size_bytes <= SAMPLE_FILE_SIZE_CAP_BYTES
                && new_file.size_bytes <= SAMPLE_FILE_SIZE_CAP_BYTES
            {
                weak_symbol_score.max(sample_similarity(
                    &disappeared.content_sample_hashes,
                    &new_file.content_sample_hashes,
                ))
            } else {
                weak_symbol_score
            }
        } else {
            symbol_score
        };

        let threshold = if old_symbols.len() <= SYMBOL_POOR_MAX_SYMBOLS
            || new_symbols.len() <= SYMBOL_POOR_MAX_SYMBOLS
        {
            SAMPLE_RENAME_THRESHOLD
        } else {
            RENAME_SYMBOL_THRESHOLD
        };
        if score >= threshold {
            matches.push((score, new_file));
        }
    }

    if matches.is_empty() {
        return Ok(None);
    }
    matches.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let best = matches[0];
    let second = matches.get(1).map(|(score, _)| *score).unwrap_or(0.0);
    if best.0 - second < RENAME_DOMINANCE_MARGIN {
        return Ok(None);
    }

    Ok(Some(IdentityResolution::Rename {
        preserved_id: disappeared.id,
        new_path: best.1.path.clone(),
    }))
}

/// Detect whether a disappeared file was split into multiple new files.
///
/// For each new file, computes Jaccard similarity of symbol sets against
/// the disappeared file. Files above the threshold are considered split targets.
/// The best-matching file becomes the primary (preserves the old ID);
/// others become secondaries.
pub(super) fn detect_split(
    disappeared: &FileNode,
    new_files: &[FileNode],
    old_symbol_sets: &HashMap<FileNodeId, HashSet<String>>,
    new_symbol_sets: &HashMap<FileNodeId, HashSet<String>>,
) -> crate::Result<Option<IdentityResolution>> {
    let old_symbols = match old_symbol_sets.get(&disappeared.id) {
        Some(s) => s,
        None => return Ok(None),
    };
    if old_symbols.is_empty() {
        return Ok(None);
    }

    let mut matches: Vec<(f64, &FileNode)> = Vec::new();
    for new_file in new_files {
        if let Some(new_syms) = new_symbol_sets.get(&new_file.id) {
            let sim = jaccard_similarity(old_symbols, new_syms);
            if sim >= SPLIT_THRESHOLD {
                matches.push((sim, new_file));
            }
        }
    }

    if matches.len() < 2 {
        // Not a split if fewer than 2 matches. A single match is a rename,
        // which is handled by content-hash detection.
        return Ok(None);
    }

    // Sort by similarity descending. Best match gets to keep the old ID.
    matches.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let primary = matches[0].1;
    let secondaries: Vec<String> = matches[1..].iter().map(|(_, f)| f.path.clone()).collect();

    Ok(Some(IdentityResolution::Split {
        primary: (disappeared.id, primary.path.clone()),
        secondaries,
    }))
}

/// Detect whether multiple disappeared files were merged into one new file.
///
/// For each new file, checks if it has high Jaccard similarity with multiple
/// disappeared files.
pub(super) fn detect_merge(
    disappeared: &[FileNode],
    new_files: &[FileNode],
    old_symbol_sets: &HashMap<FileNodeId, HashSet<String>>,
    new_symbol_sets: &HashMap<FileNodeId, HashSet<String>>,
) -> crate::Result<Vec<IdentityResolution>> {
    let mut resolutions = Vec::new();

    for new_file in new_files {
        let new_symbols = match new_symbol_sets.get(&new_file.id) {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };

        let mut merged_ids: Vec<FileNodeId> = Vec::new();
        for old_file in disappeared {
            if let Some(old_syms) = old_symbol_sets.get(&old_file.id) {
                let sim = jaccard_similarity(old_syms, new_symbols);
                if sim >= MERGE_THRESHOLD {
                    merged_ids.push(old_file.id);
                }
            }
        }

        if merged_ids.len() >= 2 {
            resolutions.push(IdentityResolution::Merge {
                new_path: new_file.path.clone(),
                old_ids: merged_ids,
            });
        }
    }

    Ok(resolutions)
}

#[cfg(test)]
mod tests;
