use std::path::Path;

use crate::core::path_safety::safe_join_in_repo;

/// Read the source body of a symbol from the file on disk.
///
/// `file_path` originates in the SQLite graph store. A poisoned store shipped
/// inside a hostile repo can contain arbitrary path strings; we refuse to
/// follow absolute or traversing paths so Deep-budget card compilation cannot
/// be turned into an arbitrary-file-read primitive.
pub(super) fn read_symbol_body(
    repo_root: Option<&Path>,
    file_path: &str,
    byte_range: (u32, u32),
) -> Option<String> {
    let root = repo_root.unwrap_or(Path::new("."));
    let full_path = safe_join_in_repo(root, file_path)?;
    let content = std::fs::read(&full_path).ok()?;
    let start = byte_range.0 as usize;
    let end = (byte_range.1 as usize).min(content.len());
    std::str::from_utf8(content.get(start..end)?)
        .ok()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn read_symbol_body_refuses_path_traversal() {
        let outer = tempdir().unwrap();
        let repo = outer.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        // Sibling file the attacker wants to exfiltrate.
        fs::write(outer.path().join("secret.txt"), b"top secret").unwrap();

        assert!(read_symbol_body(Some(&repo), "../secret.txt", (0, 10)).is_none());
    }

    #[test]
    fn read_symbol_body_refuses_absolute_path() {
        let repo = tempdir().unwrap();
        let root = repo.path().to_path_buf();
        // On Unix `/etc/passwd` is the canonical probe. On Windows `is_absolute`
        // also refuses `\\server\share\x` and `C:\...`, covered by unit tests
        // in core::path_safety.
        assert!(read_symbol_body(Some(&root), "/etc/passwd", (0, 10)).is_none());
    }

    #[test]
    fn read_symbol_body_reads_in_repo_files() {
        let repo = tempdir().unwrap();
        let file_path = repo.path().join("src").join("lib.rs");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, b"fn ok() {}").unwrap();

        let got = read_symbol_body(Some(repo.path()), "src/lib.rs", (0, 10)).unwrap();
        assert_eq!(got, "fn ok() {}");
    }
}
