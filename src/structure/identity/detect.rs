//! Symbol-set similarity detection for split/merge rename cases.

use std::collections::{HashMap, HashSet};

use super::IdentityResolution;
use crate::core::ids::FileNodeId;
use crate::structure::graph::{FileNode, GraphStore};

/// Jaccard similarity threshold for split detection (one -> many).
const SPLIT_THRESHOLD: f64 = 0.4;
/// Jaccard similarity threshold for merge detection (many -> one).
const MERGE_THRESHOLD: f64 = 0.5;

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
mod tests {
    use super::*;

    #[test]
    fn jaccard_similarity_identical_sets() {
        let a: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
        let b: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_similarity_disjoint_sets() {
        let a: HashSet<String> = ["a".to_string()].into_iter().collect();
        let b: HashSet<String> = ["b".to_string()].into_iter().collect();
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_similarity_partial_overlap() {
        let a: HashSet<String> = ["a".to_string(), "b".to_string(), "c".to_string()]
            .into_iter()
            .collect();
        let b: HashSet<String> = ["a".to_string(), "b".to_string(), "d".to_string()]
            .into_iter()
            .collect();
        // Intersection = {a, b} = 2, Union = {a, b, c, d} = 4, sim = 0.5
        assert!((jaccard_similarity(&a, &b) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_similarity_empty_both() {
        let a: HashSet<String> = HashSet::new();
        let b: HashSet<String> = HashSet::new();
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_similarity_one_empty() {
        let a: HashSet<String> = ["a".to_string()].into_iter().collect();
        let b: HashSet<String> = HashSet::new();
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
    }
}
