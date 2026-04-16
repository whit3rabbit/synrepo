//! Git-backed rename detection for the identity cascade (stage 6 step 4).
//!
//! Uses `gix` tree diff with rewrite tracking enabled to detect renames that
//! content-hash and symbol-set matching missed.

use gix::object::tree::diff::Change;

/// Detect renames by diffing the HEAD commit against its first parent with
/// rewrite tracking enabled. Returns `(old_path, new_path)` pairs.
///
/// This is a fallback for the identity cascade: it only runs when content-hash
/// rename, split detection, and merge detection all fail to match a disappeared
/// file. The function looks at the most recent commit only, which covers the
/// common case of a single-commit rename.
pub fn detect_recent_renames(repo: &gix::Repository) -> crate::Result<Vec<(String, String)>> {
    let head_id = match repo.head_id() {
        Ok(id) => id,
        Err(_) => return Ok(Vec::new()),
    };
    let head_commit = head_id
        .object()
        .map_err(|e| crate::Error::Git(e.to_string()))?
        .into_commit();

    let current_tree = head_commit
        .tree()
        .map_err(|e| crate::Error::Git(e.to_string()))?;
    let parent_tree = match head_commit.parent_ids().next() {
        Some(pid) => pid
            .object()
            .map_err(|e| crate::Error::Git(e.to_string()))?
            .into_commit()
            .tree()
            .map_err(|e| crate::Error::Git(e.to_string()))?,
        None => return Ok(Vec::new()),
    };

    let mut renames = Vec::new();
    let mut platform = parent_tree
        .changes()
        .map_err(|e| crate::Error::Git(e.to_string()))?;
    // Enable rewrite tracking with default similarity threshold.
    platform.options(|opts| {
        opts.track_path();
        opts.track_rewrites(Some(Default::default()));
    });
    platform
        .for_each_to_obtain_tree(&current_tree, |change| {
            if let Change::Rewrite {
                source_location,
                location,
                entry_mode,
                ..
            } = change
            {
                if entry_mode.is_no_tree() {
                    let old = String::from_utf8_lossy(source_location.as_ref()).into_owned();
                    let new = String::from_utf8_lossy(location.as_ref()).into_owned();
                    renames.push((old, new));
                }
            }
            Ok::<_, crate::Error>(std::ops::ControlFlow::Continue(()))
        })
        .map_err(|e| crate::Error::Git(e.to_string()))?;

    Ok(renames)
}
