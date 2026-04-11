use std::path::Path;

use crate::core::provenance::{Provenance, SourceRef};

/// Build a `Provenance` record for a structural-pipeline row.
pub(super) fn make_provenance(
    pass: &str,
    revision: &str,
    path: &str,
    content_hash: &str,
) -> Provenance {
    Provenance::structural(
        pass,
        revision,
        vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: content_hash.to_string(),
        }],
    )
}

/// Read the current git HEAD SHA for provenance records.
///
/// Returns "unknown" if the repository has no git history or the HEAD
/// file cannot be resolved (for example freshly initialised temp repos in tests).
pub(super) fn current_git_revision(repo_root: &Path) -> String {
    let head_path = repo_root.join(".git/HEAD");
    let Ok(head) = std::fs::read_to_string(&head_path) else {
        return "unknown".to_string();
    };
    let head = head.trim();

    if let Some(ref_path) = head.strip_prefix("ref: ") {
        let ref_file = repo_root.join(".git").join(ref_path);
        if let Ok(sha) = std::fs::read_to_string(&ref_file) {
            let sha = sha.trim().to_string();
            if !sha.is_empty() {
                return sha;
            }
        }
        return "unknown".to_string();
    }

    if head.len() >= 7 {
        return head.to_string();
    }

    "unknown".to_string()
}
