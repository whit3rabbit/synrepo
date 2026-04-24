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

mod detect;
mod persist;

pub use persist::persist_resolutions;

use detect::{detect_merge, detect_split, symbol_set_for_file};

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
