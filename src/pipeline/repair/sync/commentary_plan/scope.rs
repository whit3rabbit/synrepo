//! Scope prefix normalization and matching for commentary work planning.

use std::path::PathBuf;

/// Convert scope `PathBuf`s into `/`-normalized, trailing-slash-terminated
/// string prefixes so a prefix-match cannot spuriously accept sibling
/// directories (`src` matching `src-extra/...`).
pub fn normalize_scope_prefixes(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            let mut s = p.to_string_lossy().replace('\\', "/");
            if !s.is_empty() && !s.ends_with('/') {
                s.push('/');
            }
            s
        })
        .collect()
}

/// True if `file_path` (stored as recorded in the graph, possibly with
/// backslashes on Windows) starts with any of the normalized prefixes.
pub fn path_matches_any_prefix(file_path: &str, prefixes: &[String]) -> bool {
    let normalized = file_path.replace('\\', "/");
    prefixes.iter().any(|p| normalized.starts_with(p.as_str()))
}

pub(super) fn in_scope(path: &str, prefixes: Option<&[String]>) -> bool {
    match prefixes {
        None => true,
        Some(p) => path_matches_any_prefix(path, p),
    }
}

pub(super) fn scan_emit_interval(total: usize) -> usize {
    match total {
        0..=20 => 1,
        _ => (total / 20).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_terminated_with_slash() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert_eq!(prefixes, vec!["src/".to_string()]);
    }

    #[test]
    fn prefix_sibling_does_not_match() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert!(path_matches_any_prefix("src/lib.rs", &prefixes));
        assert!(!path_matches_any_prefix("src-extra/lib.rs", &prefixes));
    }

    #[test]
    fn backslash_paths_match_forward_slash_prefix() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert!(path_matches_any_prefix("src\\lib.rs", &prefixes));
    }

    #[test]
    fn empty_scope_matches_nothing() {
        let prefixes = normalize_scope_prefixes(&[]);
        assert!(!path_matches_any_prefix("src/lib.rs", &prefixes));
    }

    #[test]
    fn nested_prefix_match() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("crates/core/src")]);
        assert!(path_matches_any_prefix("crates/core/src/lib.rs", &prefixes));
        assert!(!path_matches_any_prefix(
            "crates/core/tests/a.rs",
            &prefixes
        ));
    }

    #[test]
    fn scan_emit_interval_scales_with_repo_size() {
        assert_eq!(scan_emit_interval(0), 1);
        assert_eq!(scan_emit_interval(20), 1);
        assert_eq!(scan_emit_interval(21), 1);
        assert_eq!(scan_emit_interval(100), 5);
        assert_eq!(scan_emit_interval(4000), 200);
    }
}
