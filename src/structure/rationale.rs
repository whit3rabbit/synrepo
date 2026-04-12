//! Inline decision-marker extraction for the structural pipeline.
//!
//! Scans code files for `// DECISION:` (or language-equivalent) line comments
//! and returns the decision text. Results are stored on `FileNode` as
//! `inline_decisions` — they cannot produce `ConceptNode` records because
//! invariant 7 restricts ConceptNode creation to human-authored markdown in
//! configured concept directories.

use crate::substrate::FileClass;

/// Extract all inline `DECISION:` markers from a code file.
///
/// Matches lines where the language-appropriate single-line comment prefix
/// is followed by a space, the exact token `DECISION:`, and non-empty text.
/// Example: `// DECISION: use SQLite because lock contention is low`.
///
/// Returns an empty vec when the file class has no known comment prefix or
/// no markers are found. `// DECISIONS:` (note the trailing `S`) is NOT
/// matched because the token must be exactly `DECISION:`.
pub fn extract_inline_decisions(content: &[u8], file_class: &FileClass) -> Vec<String> {
    let prefix = match comment_prefix(file_class) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let text = match std::str::from_utf8(content) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    // Marker: `<prefix> DECISION: <non-empty text>`
    let marker = format!("{prefix} DECISION: ");
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix(&marker)
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        })
        .collect()
}

/// Return the single-line comment prefix for a supported language.
fn comment_prefix(file_class: &FileClass) -> Option<&'static str> {
    match file_class {
        FileClass::SupportedCode { language } => match *language {
            "rust" | "typescript" | "tsx" => Some("//"),
            "python" => Some("#"),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_class() -> FileClass {
        FileClass::SupportedCode { language: "rust" }
    }
    fn python_class() -> FileClass {
        FileClass::SupportedCode { language: "python" }
    }
    fn ts_class() -> FileClass {
        FileClass::SupportedCode {
            language: "typescript",
        }
    }

    #[test]
    fn rust_decision_marker_extracted() {
        let src =
            b"fn main() {}\n// DECISION: use SQLite because contention is low\nfn other() {}\n";
        let decisions = extract_inline_decisions(src, &rust_class());
        assert_eq!(decisions, vec!["use SQLite because contention is low"]);
    }

    #[test]
    fn python_decision_marker_extracted() {
        let src = b"# DECISION: avoid threads due to GIL\ndef foo(): pass\n";
        let decisions = extract_inline_decisions(src, &python_class());
        assert_eq!(decisions, vec!["avoid threads due to GIL"]);
    }

    #[test]
    fn typescript_decision_marker_extracted() {
        let src =
            b"// DECISION: use React hooks over class components\nexport const App = () => null;\n";
        let decisions = extract_inline_decisions(src, &ts_class());
        assert_eq!(decisions, vec!["use React hooks over class components"]);
    }

    #[test]
    fn decisions_plural_is_not_matched() {
        // `// DECISIONS:` must NOT match — the exact token is `DECISION:`.
        let src = b"// DECISIONS: some note\n// DECISION: this matches\n";
        let decisions = extract_inline_decisions(src, &rust_class());
        assert_eq!(decisions, vec!["this matches"]);
    }

    #[test]
    fn no_markers_returns_empty() {
        let src = b"fn foo() {}\n// regular comment\n";
        assert!(extract_inline_decisions(src, &rust_class()).is_empty());
    }

    #[test]
    fn unsupported_class_returns_empty() {
        let src = b"// DECISION: ignored\n";
        assert!(extract_inline_decisions(src, &FileClass::TextCode).is_empty());
        assert!(extract_inline_decisions(src, &FileClass::Markdown).is_empty());
    }

    #[test]
    fn multiple_markers_all_extracted() {
        let src = b"// DECISION: first choice\nfn a() {}\n// DECISION: second choice\n";
        let decisions = extract_inline_decisions(src, &rust_class());
        assert_eq!(decisions, vec!["first choice", "second choice"]);
    }
}
