use std::path::{Path, PathBuf};

/// Read the source body of a symbol from the file on disk.
pub(super) fn read_symbol_body(
    repo_root: &Option<PathBuf>,
    file_path: &str,
    byte_range: (u32, u32),
) -> Option<String> {
    let root = repo_root.as_deref().unwrap_or(Path::new("."));
    let full_path = root.join(file_path);
    let content = std::fs::read(&full_path).ok()?;
    let start = byte_range.0 as usize;
    let end = (byte_range.1 as usize).min(content.len());
    std::str::from_utf8(content.get(start..end)?)
        .ok()
        .map(str::to_string)
}
