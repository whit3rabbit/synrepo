//! Stage 6: rename detection cascade (split / merge / git-rename).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use crate::{
    core::ids::FileNodeId,
    pipeline::git::{detect_recent_renames, open_repo},
    structure::{graph::GraphStore, identity},
    substrate::DiscoveredFile,
};

/// Stage 6: run the identity cascade (split/merge/git-rename detection) for
/// files that disappeared but were not matched by content-hash rename.
///
/// Returns the number of SplitFrom/MergedFrom edges written.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_identity_cascade(
    graph: &mut dyn GraphStore,
    discovered_paths: &BTreeSet<String>,
    existing_file_paths: &[(String, FileNodeId)],
    rename_matched_old_paths: &mut HashSet<String>,
    _discovered: &[DiscoveredFile],
    revision: &str,
    identities_resolved: &mut usize,
    repo_root: &Path,
) -> crate::Result<usize> {
    // Collect disappeared files not already matched by content-hash rename.
    let mut disappeared = Vec::new();
    for (path, _) in existing_file_paths {
        if !discovered_paths.contains(path) && !rename_matched_old_paths.contains(path) {
            if let Some(node) = graph.file_by_path(path)? {
                disappeared.push(node);
            }
        }
    }

    if disappeared.is_empty() {
        return Ok(0);
    }

    // Collect new files (paths that weren't in existing but are in discovered).
    let existing_path_set: HashSet<&str> = existing_file_paths
        .iter()
        .map(|(p, _)| p.as_str())
        .collect();
    let mut new_files = Vec::new();
    for path in discovered_paths
        .iter()
        .filter(|p| !existing_path_set.contains(p.as_str()))
    {
        if let Some(node) = graph.file_by_path(path)? {
            new_files.push(node);
        }
    }

    if new_files.is_empty() {
        return Ok(0);
    }

    // Attempt git rename detection for use as step 4 fallback.
    let git_renames = match open_repo(repo_root) {
        Ok(repo) => match detect_recent_renames(&repo) {
            Ok(renames) => {
                let map = HashMap::<String, String>::from_iter(renames);
                if map.is_empty() {
                    None
                } else {
                    Some(map)
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "git rename detection failed; skipping step 4");
                None
            }
        },
        Err(_) => None,
    };

    let resolutions =
        identity::resolve_identities(&disappeared, &new_files, graph, git_renames.as_ref())?;
    *identities_resolved += resolutions.len();

    // Protect git-renamed old paths from deletion by delete_missing_files.
    let git_rename_ids: HashSet<_> = resolutions
        .iter()
        .filter_map(|r| match r {
            identity::IdentityResolution::GitRename { preserved_id, .. } => Some(*preserved_id),
            _ => None,
        })
        .collect();
    for old_file in &disappeared {
        if git_rename_ids.contains(&old_file.id) {
            rename_matched_old_paths.insert(old_file.path.clone());
        }
    }

    let edges = identity::persist_resolutions(&resolutions, graph, revision)?;
    Ok(edges)
}
