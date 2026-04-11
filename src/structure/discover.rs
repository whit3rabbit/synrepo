//! Filesystem discovery: walk the configured roots and classify files.
//!
//! Respects `.gitignore`, `.git/info/exclude`, and synrepo's own `.synignore`.
//! This module is the repo-local source of truth for discovery and file
//! classification even though `syntext` still owns the current index-build
//! walk until lexical-substrate task group 2 is implemented.

use crate::config::Config;
use ignore::{
    gitignore::{Gitignore, GitignoreBuilder},
    WalkBuilder,
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const SNIFF_BYTES: usize = 8 * 1024;
const LFS_POINTER_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1\n";

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
/// yet integrate with git worktrees or submodules, which remains phase 1.
pub fn discover(repo_root: &Path, config: &Config) -> crate::Result<Vec<DiscoveredFile>> {
    let redaction_matcher = build_redaction_matcher(repo_root, &config.redact_globs)?;
    let mut discovered = BTreeMap::new();
    let mut walker = WalkBuilder::new(repo_root);
    walker.hidden(false);
    walker.git_ignore(true);
    walker.git_exclude(true);
    walker.git_global(true);
    walker.require_git(false);
    walker.follow_links(false);
    walker.add_custom_ignore_filename(".synignore");

    for result in walker.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let absolute_path = entry.into_path();
        let relative_path = match absolute_path.strip_prefix(repo_root) {
            Ok(path) => path.to_path_buf(),
            Err(_) => continue,
        };
        if !is_within_configured_roots(&relative_path, &config.roots) {
            continue;
        }

        let size_bytes = match absolute_path.metadata() {
            Ok(metadata) => metadata.len(),
            Err(_) => continue,
        };
        let is_redacted = redaction_matcher
            .matched_path_or_any_parents(&relative_path, false)
            .is_ignore();

        let class = if size_bytes > config.max_file_size_bytes || is_redacted {
            classify_candidate(&relative_path, size_bytes, &[], config, is_redacted)
        } else {
            let first_bytes = read_file_head(&absolute_path)?;
            classify_candidate(
                &relative_path,
                size_bytes,
                &first_bytes,
                config,
                is_redacted,
            )
        };

        if matches!(class, FileClass::Skipped(_)) {
            continue;
        }

        let relative_path = normalize_relative_path(&relative_path);
        discovered
            .entry(relative_path.clone())
            .or_insert(DiscoveredFile {
                absolute_path,
                relative_path,
                class,
                size_bytes,
            });
    }

    Ok(discovered.into_values().collect())
}

/// Classify a single file's content shape and support tier.
///
/// `discover()` layers repository policy such as size caps and redaction
/// globs on top of this classifier before admitting files to the discovered
/// set. This function stays focused on deterministic content and path-based
/// classification so it can be tested in isolation.
pub fn classify(path: &Path, size_bytes: u64, first_bytes: &[u8]) -> FileClass {
    if size_bytes == 0 {
        return FileClass::Skipped(SkipReason::Empty);
    }
    if first_bytes.starts_with(LFS_POINTER_PREFIX) {
        return FileClass::Skipped(SkipReason::LfsPointer);
    }
    match sniff_text_content(first_bytes) {
        Ok(()) => {}
        Err(reason) => return FileClass::Skipped(reason),
    }

    match lower_extension(path).as_deref() {
        Some("rs") => FileClass::SupportedCode { language: "rust" },
        Some("py") => FileClass::SupportedCode { language: "python" },
        Some("ts") => FileClass::SupportedCode {
            language: "typescript",
        },
        Some("tsx") => FileClass::SupportedCode { language: "tsx" },
        Some("md" | "mdx" | "markdown") => FileClass::Markdown,
        Some("ipynb") => FileClass::Jupyter,
        _ => FileClass::TextCode,
    }
}

fn classify_candidate(
    path: &Path,
    size_bytes: u64,
    first_bytes: &[u8],
    config: &Config,
    is_redacted: bool,
) -> FileClass {
    if size_bytes > config.max_file_size_bytes {
        return FileClass::Skipped(SkipReason::TooLarge);
    }
    if is_redacted {
        return FileClass::Skipped(SkipReason::Redacted);
    }
    classify(path, size_bytes, first_bytes)
}

fn build_redaction_matcher(repo_root: &Path, globs: &[String]) -> crate::Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(repo_root);
    for glob in globs {
        builder.add_line(None, glob).map_err(|err| {
            crate::Error::Config(format!("invalid redaction glob `{glob}`: {err}"))
        })?;
    }
    builder
        .build()
        .map_err(|err| crate::Error::Config(format!("invalid redaction matcher: {err}")))
}

fn read_file_head(path: &Path) -> crate::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; SNIFF_BYTES];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

fn lower_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_within_configured_roots(path: &Path, roots: &[String]) -> bool {
    roots.iter().any(|root| {
        if root == "." || root.is_empty() {
            return true;
        }
        let root_path = Path::new(root);
        path == root_path || path.starts_with(root_path)
    })
}

fn sniff_text_content(first_bytes: &[u8]) -> Result<(), SkipReason> {
    if first_bytes.contains(&0) {
        return Err(SkipReason::Binary);
    }
    if std::str::from_utf8(first_bytes).is_ok() {
        return Ok(());
    }
    if looks_binary(first_bytes) {
        return Err(SkipReason::Binary);
    }
    Err(SkipReason::UnknownEncoding)
}

fn looks_binary(first_bytes: &[u8]) -> bool {
    let suspicious = first_bytes
        .iter()
        .filter(|byte| matches!(byte, 0x01..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F))
        .count();
    suspicious * 5 >= first_bytes.len().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn classify_maps_supported_and_indexed_file_classes() {
        assert_eq!(
            classify(Path::new("src/lib.rs"), 12, b"fn example() {}"),
            FileClass::SupportedCode { language: "rust" }
        );
        assert_eq!(
            classify(Path::new("src/app.py"), 11, b"def app():\n"),
            FileClass::SupportedCode { language: "python" }
        );
        assert_eq!(
            classify(Path::new("web/app.ts"), 18, b"export const app = 1;"),
            FileClass::SupportedCode {
                language: "typescript"
            }
        );
        assert_eq!(
            classify(
                Path::new("web/app.tsx"),
                22,
                b"export const App = () => null;"
            ),
            FileClass::SupportedCode { language: "tsx" }
        );
        assert_eq!(
            classify(Path::new("README.md"), 8, b"# hello\n"),
            FileClass::Markdown
        );
        assert_eq!(
            classify(Path::new("notebook.ipynb"), 2, b"{}"),
            FileClass::Jupyter
        );
        assert_eq!(
            classify(Path::new("config.yaml"), 12, b"key: value\n"),
            FileClass::TextCode
        );
        assert_eq!(
            classify(Path::new("Makefile"), 16, b"all:\n\tcargo test\n"),
            FileClass::TextCode
        );
    }

    #[test]
    fn classify_reports_explicit_skip_reasons() {
        let config = Config {
            max_file_size_bytes: 4,
            ..Config::default()
        };

        assert_eq!(
            classify_candidate(
                Path::new("secret.env"),
                10,
                b"SECRET=1\n",
                &Config::default(),
                true,
            ),
            FileClass::Skipped(SkipReason::Redacted)
        );
        assert_eq!(
            classify_candidate(Path::new("big.txt"), 5, b"abcde", &config, false),
            FileClass::Skipped(SkipReason::TooLarge)
        );
        assert_eq!(
            classify(Path::new("empty.txt"), 0, b""),
            FileClass::Skipped(SkipReason::Empty)
        );
        assert_eq!(
            classify(
                Path::new("pointer.bin"),
                128,
                b"version https://git-lfs.github.com/spec/v1\noid sha256:abc\n"
            ),
            FileClass::Skipped(SkipReason::LfsPointer)
        );
        assert_eq!(
            classify(Path::new("blob.bin"), 4, &[0, 159, 146, 150]),
            FileClass::Skipped(SkipReason::Binary)
        );
        assert_eq!(
            classify(Path::new("latin1.txt"), 3, &[0xE9, b'l', b'e']),
            FileClass::Skipped(SkipReason::UnknownEncoding)
        );
    }

    #[test]
    fn discover_respects_roots_gitignore_and_redaction() {
        let repo = tempdir().unwrap();
        fs::write(repo.path().join(".gitignore"), "src/ignored.rs\n").unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();

        fs::write(repo.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
        fs::write(repo.path().join("src/ignored.rs"), "pub fn ignored() {}\n").unwrap();
        fs::write(repo.path().join("docs/guide.md"), "# guide\n").unwrap();
        fs::write(repo.path().join("docs/app.env"), "SECRET=1\n").unwrap();
        fs::write(repo.path().join("top.txt"), "outside configured roots\n").unwrap();

        let config = Config {
            roots: vec!["src".to_string(), "docs".to_string()],
            ..Config::default()
        };

        let discovered = discover(repo.path(), &config).unwrap();
        let relative_paths: Vec<_> = discovered
            .into_iter()
            .map(|file| file.relative_path)
            .collect();

        assert_eq!(
            relative_paths,
            vec!["docs/guide.md".to_string(), "src/lib.rs".to_string()]
        );
    }

    #[test]
    fn discover_skips_non_text_and_oversized_files() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();

        fs::write(repo.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
        fs::write(repo.path().join("src/blob.bin"), [0, 159, 146, 150]).unwrap();
        fs::write(repo.path().join("src/empty.txt"), "").unwrap();
        fs::write(
            repo.path().join("src/big.txt"),
            "abcdefghijklmnopqrstuvwxyz",
        )
        .unwrap();

        let config = Config {
            max_file_size_bytes: 20,
            ..Config::default()
        };

        let discovered = discover(repo.path(), &config).unwrap();
        let relative_paths: Vec<_> = discovered
            .into_iter()
            .map(|file| file.relative_path)
            .collect();

        assert_eq!(relative_paths, vec!["src/lib.rs".to_string()]);
    }
}
