//! Filesystem discovery: walk the configured roots and classify files.
//!
//! Respects `.gitignore`, `.git/info/exclude`, and synrepo's own `.synignore`.
//! See `synrepo-design-v4.md` section "File type handling" and
//! "Git integration gotchas" for the full list of edge cases.

use crate::config::Config;
use std::path::{Path, PathBuf};

/// Classification of a discovered file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileClass {
    /// Source code in a language we have a tree-sitter grammar for.
    SupportedCode {
        /// Language identifier (e.g. "rust", "python", "typescript", "tsx").
        language: &'static str,
    },
    /// Other text code: toml, yaml, sql, shell, etc. Indexed but not parsed for symbols.
    TextCode,
    /// Markdown or mdx. Indexed and link-parsed.
    Markdown,
    /// Jupyter notebook. Source cells extracted.
    Jupyter,
    /// Not a supported type; skipped.
    Skipped(SkipReason),
}

/// Why a file was skipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkipReason {
    /// Binary content (failed UTF-8 sniff).
    Binary,
    /// Exceeds the configured max file size.
    TooLarge,
    /// Matches a redaction glob.
    Redacted,
    /// Git LFS pointer file.
    LfsPointer,
    /// Unsupported encoding.
    UnknownEncoding,
    /// Empty file.
    Empty,
}

/// A file that the discovery pass decided is worth processing.
#[derive(Clone, Debug)]
pub struct DiscoveredFile {
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Path relative to the repo root.
    pub relative_path: String,
    /// Classification.
    pub class: FileClass,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Walk the configured roots and yield classified files.
///
/// Phase 0 implementation: honors `.gitignore` via the `ignore` crate,
/// applies size cap, applies redaction globs, sniffs encoding. Does not
/// yet integrate with git worktrees or submodules — that's phase 1.
pub fn discover(repo_root: &Path, _config: &Config) -> crate::Result<Vec<DiscoveredFile>> {
    // TODO(phase-0): implement using `ignore::WalkBuilder` with:
    //   - .gitignore respect
    //   - max_file_size filter
    //   - redaction glob filter
    //   - UTF-8 sniff (small read of file head)
    //   - LFS pointer detection (first 128 bytes = "version https://git-lfs.github.com/spec/v1\n")
    //   - symlink loop detection
    let _ = repo_root;
    Ok(Vec::new())
}

/// Classify a single path. Exposed for testing the classifier in isolation.
pub fn classify(_path: &Path, _size_bytes: u64, _first_bytes: &[u8]) -> FileClass {
    // TODO(phase-0): look up extension in a static table; fall back to
    // content sniffing for extensionless files.
    FileClass::Skipped(SkipReason::Binary)
}