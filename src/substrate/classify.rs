//! File classification: maps a discovered file to its content tier.
//!
//! This is the admission gate for the substrate layer. `discover()` calls
//! `classify_candidate()` with repository policy (size caps, redaction) already
//! applied; `classify()` is the pure content-and-path classifier that can be
//! tested in isolation.

use crate::{config::Config, core::source_language::language_label_for_extension};
use std::path::Path;

const SNIFF_BYTES: usize = 8 * 1024;
pub(super) const LFS_POINTER_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1\n";

/// The number of bytes read from each file for encoding and content sniffing.
pub(super) const SNIFF_HEAD_BYTES: usize = SNIFF_BYTES;

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
        Some(ext) => match language_label_for_extension(ext) {
            Some(language) => FileClass::SupportedCode { language },
            None => match ext {
                "md" | "mdx" | "markdown" => FileClass::Markdown,
                "ipynb" => FileClass::Jupyter,
                _ => FileClass::TextCode,
            },
        },
        _ => FileClass::TextCode,
    }
}

/// Apply repository policy (size caps, redaction) on top of `classify()`.
///
/// Called by `discover()` after size and redaction checks are resolved.
pub(super) fn classify_candidate(
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

pub(super) fn sniff_text_content(first_bytes: &[u8]) -> Result<(), SkipReason> {
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

pub(super) fn lower_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
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
            classify(Path::new("lib/main.dart"), 14, b"void main() {}"),
            FileClass::SupportedCode { language: "dart" }
        );
        assert_eq!(
            classify(Path::new("web/app.jsx"), 24, b"export function App() {}"),
            FileClass::SupportedCode {
                language: "javascript"
            }
        );
        assert_eq!(
            classify(
                Path::new("ios/AppDelegate.swift"),
                20,
                b"class AppDelegate {}"
            ),
            FileClass::SupportedCode { language: "swift" }
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
}
