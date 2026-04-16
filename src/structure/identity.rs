//! File and symbol identity resolution (stage 6).
//!
//! The rename detection cascade resolves what happened to files that
//! disappeared between compile cycles. The cascade runs in order:
//!
//! 1. Content-hash rename (exact content match at new path)
//! 2. Symbol-set split (one file's symbols spread across multiple new files)
//! 3. Symbol-set merge (multiple files' symbols consolidated into one new file)
//! 4. Git rename fallback (not yet wired, future work)
//! 5. Breakage (no match found, treat as delete + add)

use std::collections::{HashMap, HashSet};

use crate::core::ids::FileNodeId;
use crate::structure::graph::{FileNode, GraphStore};

/// Result of the rename detection cascade for one "disappeared" file.
#[derive(Clone, Debug)]
pub enum IdentityResolution {
    /// Exact rename: one new file has substantially overlapping symbols.
    /// Preserve the old node ID, append the new path to path history.
    Rename {
        /// The file node ID to preserve.
        preserved_id: FileNodeId,
        /// The new path to append.
        new_path: String,
    },
    /// Split: the symbols are distributed across multiple new files.
    /// Preserve the old node ID on the largest-overlap new file and create
    /// new nodes for the rest with `split_from` provenance edges.
    Split {
        /// The preserved node ID and its new path.
        primary: (FileNodeId, String),
        /// Additional new paths to create as new nodes with `split_from` edges.
        secondaries: Vec<String>,
    },
    /// Merge: multiple old files' symbols are in one new file.
    /// Create a new node and add `merged_from` edges from the old nodes.
    Merge {
        /// The new file's path.
        new_path: String,
        /// Old file IDs to mark as merged into the new one.
        old_ids: Vec<FileNodeId>,
    },
    /// Git rename fallback: symbol evidence was inconclusive but
    /// `git log --follow` identified a rename.
    GitRename {
        /// The file node ID to preserve.
        preserved_id: FileNodeId,
        /// The new path to append.
        new_path: String,
    },
    /// No identity could be resolved. Treat as delete + add.
    Breakage {
        /// The old file ID that lost its home.
        orphaned: FileNodeId,
        /// Reason for logging.
        reason: String,
    },
}

/// Jaccard similarity threshold for split detection (one -> many).
const SPLIT_THRESHOLD: f64 = 0.4;
/// Jaccard similarity threshold for merge detection (many -> one).
const MERGE_THRESHOLD: f64 = 0.5;

/// Collect the set of qualified symbol names defined in a file.
pub fn symbol_set_for_file(
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
fn detect_split(
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
fn detect_merge(
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

/// Run the rename detection cascade for one compile cycle.
///
/// Given the set of files that disappeared and the set of new files in this
/// cycle, resolve identities using (1) content-hash rename, (2) split,
/// (3) merge, (4) git rename, (5) breakage.
///
/// Content-hash rename is already handled in the pipeline stages (via
/// `disappeared_by_hash`). This function handles the remaining cases.
///
/// `git_renames` is an optional map of `(old_path, new_path)` pairs from
/// git's rename detection. When `None`, step 4 is skipped entirely.
pub fn resolve_identities(
    disappeared: &[FileNode],
    new_files: &[FileNode],
    graph: &dyn GraphStore,
    git_renames: Option<&HashMap<String, String>>,
) -> crate::Result<Vec<IdentityResolution>> {
    if disappeared.is_empty() || new_files.is_empty() {
        return Ok(Vec::new());
    }

    // Pre-compute symbol sets for new files (used by split and merge detection).
    let mut new_symbol_sets: HashMap<FileNodeId, HashSet<String>> = HashMap::new();
    for new_file in new_files {
        if let Ok(syms) = symbol_set_for_file(new_file.id, graph) {
            if !syms.is_empty() {
                new_symbol_sets.insert(new_file.id, syms);
            }
        }
    }

    // Pre-compute symbol sets for old files (used by merge detection).
    let mut old_symbol_sets: HashMap<FileNodeId, HashSet<String>> = HashMap::new();
    for old_file in disappeared {
        if let Ok(syms) = symbol_set_for_file(old_file.id, graph) {
            if !syms.is_empty() {
                old_symbol_sets.insert(old_file.id, syms);
            }
        }
    }

    let mut resolutions = Vec::new();
    let mut consumed_old: HashSet<FileNodeId> = HashSet::new();
    let mut consumed_new: HashSet<FileNodeId> = HashSet::new();

    // Step 2: Split detection (one old -> many new).
    for old_file in disappeared {
        if consumed_old.contains(&old_file.id) {
            continue;
        }
        if let Some(resolution) =
            detect_split(old_file, new_files, &old_symbol_sets, &new_symbol_sets)?
        {
            if let IdentityResolution::Split {
                primary: (_, ref primary_path),
                ref secondaries,
            } = &resolution
            {
                consumed_old.insert(old_file.id);
                // Find the primary new file and mark it consumed.
                if let Some(nf) = new_files.iter().find(|f| f.path == *primary_path) {
                    consumed_new.insert(nf.id);
                }
                for sec_path in secondaries {
                    if let Some(nf) = new_files.iter().find(|f| f.path == *sec_path) {
                        consumed_new.insert(nf.id);
                    }
                }
            }
            resolutions.push(resolution);
        }
    }

    // Step 3: Merge detection (many old -> one new).
    let unconsumed_old: Vec<FileNode> = disappeared
        .iter()
        .filter(|f| !consumed_old.contains(&f.id))
        .cloned()
        .collect();
    let unconsumed_new: Vec<FileNode> = new_files
        .iter()
        .filter(|f| !consumed_new.contains(&f.id))
        .cloned()
        .collect();

    let merge_resolutions = detect_merge(
        &unconsumed_old,
        &unconsumed_new,
        &old_symbol_sets,
        &new_symbol_sets,
    )?;
    for resolution in &merge_resolutions {
        if let IdentityResolution::Merge {
            ref old_ids,
            ref new_path,
        } = resolution
        {
            for id in old_ids {
                consumed_old.insert(*id);
            }
            if let Some(nf) = new_files.iter().find(|f| f.path == *new_path) {
                consumed_new.insert(nf.id);
            }
        }
    }
    resolutions.extend(merge_resolutions);

    // Step 4: Git rename fallback. When symbol-set evidence was inconclusive,
    // check if git detected a rename for the disappeared file.
    if let Some(renames) = git_renames {
        let unconsumed_after_merge: Vec<FileNode> = disappeared
            .iter()
            .filter(|f| !consumed_old.contains(&f.id))
            .cloned()
            .collect();

        for old_file in &unconsumed_after_merge {
            if let Some(new_path) = renames.get(&old_file.path) {
                // Verify the new path actually exists in the discovered set.
                if new_files.iter().any(|f| f.path == *new_path) {
                    consumed_old.insert(old_file.id);
                    if let Some(nf) = new_files.iter().find(|f| f.path == *new_path) {
                        consumed_new.insert(nf.id);
                    }
                    resolutions.push(IdentityResolution::GitRename {
                        preserved_id: old_file.id,
                        new_path: new_path.clone(),
                    });
                }
            }
        }
    }

    // Step 5: Breakage for anything not matched.
    for old_file in disappeared {
        if !consumed_old.contains(&old_file.id) {
            resolutions.push(IdentityResolution::Breakage {
                orphaned: old_file.id,
                reason: "no symbol-set overlap with any new file".to_string(),
            });
        }
    }

    Ok(resolutions)
}

/// Persist identity resolutions to the graph by writing the appropriate edges
/// and updating path history. Returns the number of edges written.
pub fn persist_resolutions(
    resolutions: &[IdentityResolution],
    graph: &mut dyn GraphStore,
    revision: &str,
) -> crate::Result<usize> {
    use crate::core::ids::NodeId;
    use crate::structure::graph::{derive_edge_id, Edge, EdgeKind, Epistemic};

    let mut edges_written = 0;

    for resolution in resolutions {
        match resolution {
            IdentityResolution::Rename {
                preserved_id,
                new_path,
            } => {
                tracing::debug!(
                    file_id = %preserved_id,
                    new_path = %new_path,
                    "identity: rename resolution (already handled in stages)"
                );
            }
            IdentityResolution::Split {
                primary: (old_id, primary_path),
                secondaries,
            } => {
                for sec_path in secondaries {
                    if let Some(sec_file) = graph.file_by_path(sec_path)? {
                        let edge = Edge {
                            id: derive_edge_id(
                                NodeId::File(sec_file.id),
                                NodeId::File(*old_id),
                                EdgeKind::SplitFrom,
                            ),
                            from: NodeId::File(sec_file.id),
                            to: NodeId::File(*old_id),
                            kind: EdgeKind::SplitFrom,
                            owner_file_id: None,
                            last_observed_rev: None,
                            retired_at_rev: None,
                            epistemic: Epistemic::ParserObserved,
                            drift_score: 0.0,
                            provenance: crate::core::provenance::Provenance::structural(
                                "identity_split",
                                revision,
                                vec![crate::core::provenance::SourceRef {
                                    file_id: Some(*old_id),
                                    path: primary_path.clone(),
                                    content_hash: String::new(),
                                }],
                            ),
                        };
                        graph.insert_edge(edge)?;
                        edges_written += 1;
                    }
                }
            }
            IdentityResolution::Merge { new_path, old_ids } => {
                if let Some(new_file) = graph.file_by_path(new_path)? {
                    for old_id in old_ids {
                        let edge = Edge {
                            id: derive_edge_id(
                                NodeId::File(new_file.id),
                                NodeId::File(*old_id),
                                EdgeKind::MergedFrom,
                            ),
                            from: NodeId::File(new_file.id),
                            to: NodeId::File(*old_id),
                            kind: EdgeKind::MergedFrom,
                            owner_file_id: None,
                            last_observed_rev: None,
                            retired_at_rev: None,
                            epistemic: Epistemic::ParserObserved,
                            drift_score: 0.0,
                            provenance: crate::core::provenance::Provenance::structural(
                                "identity_merge",
                                revision,
                                vec![crate::core::provenance::SourceRef {
                                    file_id: Some(*old_id),
                                    path: new_path.clone(),
                                    content_hash: String::new(),
                                }],
                            ),
                        };
                        graph.insert_edge(edge)?;
                        edges_written += 1;
                    }

                    // Append old file paths to the new file's path_history.
                    let mut updated = new_file.clone();
                    for old_id in old_ids {
                        if let Some(old_file) = graph.get_file(*old_id)? {
                            updated.path_history.push(old_file.path.clone());
                        }
                    }
                    if updated.path_history.len() != new_file.path_history.len() {
                        graph.upsert_file(updated)?;
                    }
                }
            }
            IdentityResolution::GitRename {
                preserved_id,
                new_path,
            } => {
                // Preserve the old node ID, update its path, and append the
                // old path to path_history.
                if let Some(mut file) = graph.get_file(*preserved_id)? {
                    let old_path = file.path.clone();
                    file.path_history.insert(0, old_path);
                    file.path = new_path.clone();
                    graph.upsert_file(file)?;
                }
                tracing::debug!(
                    file_id = %preserved_id,
                    new_path = %new_path,
                    "identity: git rename resolution"
                );
            }
            IdentityResolution::Breakage { orphaned, reason } => {
                tracing::info!(
                    file_id = %orphaned,
                    reason = %reason,
                    "identity: breakage resolution (file deleted without match)"
                );
            }
        }
    }

    Ok(edges_written)
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
