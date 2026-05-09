//! Shared source-language metadata for discovery and parser dispatch.

/// Source language known to synrepo's structural parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLanguageSpec {
    /// Stable lowercase language label stored on `FileNode.language`.
    pub label: &'static str,
    /// File extensions, without a leading dot, that map to this language.
    pub extensions: &'static [&'static str],
}

/// Every tree-sitter-backed language currently wired into synrepo.
pub const SOURCE_LANGUAGE_SPECS: &[SourceLanguageSpec] = &[
    SourceLanguageSpec {
        label: "rust",
        extensions: &["rs"],
    },
    SourceLanguageSpec {
        label: "python",
        extensions: &["py"],
    },
    SourceLanguageSpec {
        label: "typescript",
        extensions: &["ts"],
    },
    SourceLanguageSpec {
        label: "tsx",
        extensions: &["tsx"],
    },
    SourceLanguageSpec {
        label: "go",
        extensions: &["go"],
    },
    SourceLanguageSpec {
        label: "javascript",
        extensions: &["js", "jsx", "mjs", "cjs"],
    },
    SourceLanguageSpec {
        label: "java",
        extensions: &["java"],
    },
    SourceLanguageSpec {
        label: "kotlin",
        extensions: &["kt", "kts"],
    },
    SourceLanguageSpec {
        label: "csharp",
        extensions: &["cs"],
    },
    SourceLanguageSpec {
        label: "php",
        extensions: &["php"],
    },
    SourceLanguageSpec {
        label: "ruby",
        extensions: &["rb"],
    },
    SourceLanguageSpec {
        label: "swift",
        extensions: &["swift"],
    },
    SourceLanguageSpec {
        label: "c",
        extensions: &["c", "h"],
    },
    SourceLanguageSpec {
        label: "cpp",
        extensions: &["cpp", "hpp", "cc", "cxx"],
    },
    SourceLanguageSpec {
        label: "dart",
        extensions: &["dart"],
    },
];

/// Resolve a file extension to a stable language label.
pub fn language_label_for_extension(ext: &str) -> Option<&'static str> {
    let ext = ext.strip_prefix('.').unwrap_or(ext);
    SOURCE_LANGUAGE_SPECS
        .iter()
        .find(|spec| {
            spec.extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        })
        .map(|spec| spec.label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_labels_resolve_case_insensitive_extensions() {
        assert_eq!(language_label_for_extension("dart"), Some("dart"));
        assert_eq!(language_label_for_extension(".JSX"), Some("javascript"));
        assert_eq!(language_label_for_extension("unknown"), None);
    }
}
