//! Path-safety helpers for untrusted relative paths.
//!
//! Inputs to these helpers arrive from two directions that are outside the
//! binary's control:
//!
//! 1. Rows in `.synrepo/graph/nodes.db` — a poisoned database shipped inside
//!    a hostile clone can contain arbitrary `file.path` strings.
//! 2. Fields in `.synrepo/config.toml` (notably `export_dir` and
//!    `semantic_model`) — the file travels with the repo, so a cloned repo
//!    can carry attacker-chosen values.
//!
//! We validate at the string level (no `canonicalize` call) so rejection
//! happens before any filesystem probe.
//!
//! Rejection rules for `safe_join_in_repo`:
//!   - empty input
//!   - absolute paths (`Path::is_absolute` returns true for `/etc/...` on
//!     Unix and for `C:\...`, `\\server\share\...`, and `\\?\...` on Windows)
//!   - any `..` component
//!   - any `Component::Prefix` (catches Windows prefixes even if some future
//!     platform loosens `is_absolute`)
//!   - embedded NUL bytes (defence against smuggled terminators)
//!   - newline characters (defence against configuration injection into .gitignore)
//!
//! `has_windows_prefix_component` is a cross-platform check that returns
//! true for UNC / verbatim / device / drive prefixes on Windows. On Unix
//! no component ever parses as `Component::Prefix`, so the function is a
//! cheap `false`.

use std::path::{Component, Path, PathBuf};

/// Join `relative` onto `root` only if `relative` is a well-formed
/// in-repo relative path. Returns `None` for anything that could escape
/// `root` or that carries an OS-specific prefix.
pub(crate) fn safe_join_in_repo(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.is_empty()
        || relative.as_bytes().contains(&0)
        || relative.contains('\n')
        || relative.contains('\r')
    {
        return None;
    }

    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return None;
    }

    for component in candidate.components() {
        match component {
            Component::ParentDir | Component::Prefix(_) => return None,
            Component::RootDir => return None,
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    Some(root.join(candidate))
}

/// Return true if `path` contains any OS-level prefix component
/// (drive letters, UNC shares, verbatim `\\?\`, device `\\.\`). On Unix
/// this is always false; on Windows it is the authoritative way to tell
/// "this path has a root prefix" without relying on byte heuristics.
///
/// Only referenced under the `semantic-triage` feature today, so
/// `#[allow(dead_code)]` keeps the default build clippy-clean.
#[allow(dead_code)]
pub(crate) fn has_windows_prefix_component(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::Prefix(_)))
}

/// String-level UNC detector that works on every platform. `Path` on Unix
/// will parse `\\server\share` as a single `Normal` component, so we also
/// look at the raw bytes for callers that want to reject UNC-looking
/// strings independent of the host OS.
///
/// Only referenced under the `semantic-triage` feature today, so
/// `#[allow(dead_code)]` keeps the default build clippy-clean.
#[allow(dead_code)]
pub(crate) fn looks_like_unc(raw: &str) -> bool {
    raw.starts_with(r"\\") || raw.starts_with("//")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_accepts_simple_relative_paths() {
        let root = Path::new("/tmp/repo");
        assert_eq!(
            safe_join_in_repo(root, "src/lib.rs"),
            Some(PathBuf::from("/tmp/repo/src/lib.rs"))
        );
        assert_eq!(
            safe_join_in_repo(root, "./docs/guide.md"),
            Some(PathBuf::from("/tmp/repo/./docs/guide.md"))
        );
    }

    #[test]
    fn safe_join_rejects_absolute_paths() {
        let root = Path::new("/tmp/repo");
        assert_eq!(safe_join_in_repo(root, "/etc/passwd"), None);
    }

    #[test]
    fn safe_join_rejects_parent_dir_components() {
        let root = Path::new("/tmp/repo");
        assert_eq!(safe_join_in_repo(root, "../secret"), None);
        assert_eq!(safe_join_in_repo(root, "foo/../../bar"), None);
        assert_eq!(safe_join_in_repo(root, "foo/..///bar"), None);
    }

    #[test]
    fn safe_join_rejects_empty_nul_and_newlines() {
        let root = Path::new("/tmp/repo");
        assert_eq!(safe_join_in_repo(root, ""), None);
        assert_eq!(safe_join_in_repo(root, "foo\0bar"), None);
        assert_eq!(safe_join_in_repo(root, "foo\nbar"), None);
        assert_eq!(safe_join_in_repo(root, "foo\rbar"), None);
    }

    #[test]
    fn looks_like_unc_catches_smb_and_unix_style() {
        assert!(looks_like_unc(r"\\server\share\m.onnx"));
        assert!(looks_like_unc("//server/share/m.onnx"));
        assert!(!looks_like_unc("model.onnx"));
        assert!(!looks_like_unc("/usr/local/m.onnx"));
    }

    #[cfg(windows)]
    #[test]
    fn safe_join_rejects_windows_prefixes() {
        let root = Path::new(r"C:\repo");
        assert_eq!(safe_join_in_repo(root, r"C:\Windows\System32"), None);
        assert_eq!(safe_join_in_repo(root, r"\\server\share\x"), None);
    }

    #[cfg(windows)]
    #[test]
    fn has_windows_prefix_detects_unc_and_drive() {
        assert!(has_windows_prefix_component(Path::new(r"\\server\share\x")));
        assert!(has_windows_prefix_component(Path::new(r"C:\repo")));
        assert!(!has_windows_prefix_component(Path::new(r"repo\src")));
    }

    #[cfg(unix)]
    #[test]
    fn has_windows_prefix_is_false_on_unix() {
        // On Unix, no component ever parses as a Prefix, even for UNC-like
        // strings — they become Normal components. The string-level
        // `looks_like_unc` is the cross-platform check.
        assert!(!has_windows_prefix_component(Path::new(
            r"\\server\share\x"
        )));
        assert!(!has_windows_prefix_component(Path::new("/etc/passwd")));
    }
}
