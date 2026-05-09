use crate::core::source_language::language_label_for_extension;
use crate::structure::graph::SymbolKind;

/// Supported languages with a tree-sitter grammar wired in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    /// Rust (`tree-sitter-rust` crate).
    Rust,
    /// Python (`tree-sitter-python` crate).
    Python,
    /// TypeScript, non-TSX (`tree-sitter-typescript::language_typescript`).
    TypeScript,
    /// TypeScript with JSX (`tree-sitter-typescript::language_tsx`).
    Tsx,
    /// Go (`tree-sitter-go` crate).
    Go,
    /// JavaScript
    JavaScript,
    /// Java
    Java,
    /// Kotlin
    Kotlin,
    /// C#
    CSharp,
    /// PHP
    Php,
    /// Ruby
    Ruby,
    /// Swift
    Swift,
    /// C
    C,
    /// C++
    Cpp,
    /// Dart
    Dart,
}

impl Language {
    /// Enumerate every supported `Language` variant.
    ///
    /// This is the single source of truth for "which languages have a wired
    /// tree-sitter grammar". Validation tests iterate this slice, so adding a
    /// new variant without updating this list fails CI compile (match in
    /// `display_name` is exhaustive) or flushes coverage gaps (fixture test).
    pub fn supported() -> &'static [Language] {
        &[
            Language::Rust,
            Language::Python,
            Language::TypeScript,
            Language::Tsx,
            Language::Go,
            Language::JavaScript,
            Language::Java,
            Language::Kotlin,
            Language::CSharp,
            Language::Php,
            Language::Ruby,
            Language::Swift,
            Language::C,
            Language::Cpp,
            Language::Dart,
        ]
    }

    /// Stable lowercase label for diagnostics and test messages.
    pub fn display_name(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::Go => "go",
            Language::JavaScript => "javascript",
            Language::Java => "java",
            Language::Kotlin => "kotlin",
            Language::CSharp => "csharp",
            Language::Php => "php",
            Language::Ruby => "ruby",
            Language::Swift => "swift",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Dart => "dart",
        }
    }

    /// Resolve a file extension to a `Language`, if supported.
    pub fn from_extension(ext: &str) -> Option<Language> {
        match language_label_for_extension(ext)? {
            "rust" => Some(Language::Rust),
            "python" => Some(Language::Python),
            "typescript" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "go" => Some(Language::Go),
            "javascript" => Some(Language::JavaScript),
            "java" => Some(Language::Java),
            "kotlin" => Some(Language::Kotlin),
            "csharp" => Some(Language::CSharp),
            "php" => Some(Language::Php),
            "ruby" => Some(Language::Ruby),
            "swift" => Some(Language::Swift),
            "c" => Some(Language::C),
            "cpp" => Some(Language::Cpp),
            "dart" => Some(Language::Dart),
            _ => None,
        }
    }

    /// Return the tree-sitter `Language` from the corresponding crate.
    pub fn tree_sitter_language(self) -> tree_sitter::Language {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::Java => tree_sitter_java::LANGUAGE.into(),
            Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Language::Swift => tree_sitter_swift::LANGUAGE.into(),
            Language::C => tree_sitter_c::LANGUAGE.into(),
            Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Language::Dart => tree_sitter_dart::LANGUAGE.into(),
        }
    }

    pub(super) fn definition_query(self) -> &'static str {
        match self {
            Language::Rust => RUST_DEFINITION_QUERY,
            Language::Python => PYTHON_DEFINITION_QUERY,
            Language::TypeScript | Language::Tsx => TS_DEFINITION_QUERY,
            Language::Go => GO_DEFINITION_QUERY,
            Language::JavaScript => JS_DEFINITION_QUERY,
            Language::Java => JAVA_DEFINITION_QUERY,
            Language::Kotlin => KOTLIN_DEFINITION_QUERY,
            Language::CSharp => CSHARP_DEFINITION_QUERY,
            Language::Php => PHP_DEFINITION_QUERY,
            Language::Ruby => RUBY_DEFINITION_QUERY,
            Language::Swift => SWIFT_DEFINITION_QUERY,
            Language::C => C_DEFINITION_QUERY,
            Language::Cpp => CPP_DEFINITION_QUERY,
            Language::Dart => DART_DEFINITION_QUERY,
        }
    }

    /// Return the fixed pattern-index -> `SymbolKind` table for this language.
    ///
    /// Exposed so validation tests can assert the table's length matches the
    /// compiled definition query's `pattern_count()` and that every slot has
    /// an explicit assignment. Runtime still uses the length-checked
    /// `kind_for_pattern()` below, which keeps the `SymbolKind::Function`
    /// fallback for forward-compatibility with untested grammar patterns.
    pub(super) fn kind_map(self) -> &'static [SymbolKind] {
        match self {
            Language::Rust => RUST_KIND_MAP,
            Language::Python => PYTHON_KIND_MAP,
            Language::TypeScript | Language::Tsx => TS_KIND_MAP,
            Language::Go => GO_KIND_MAP,
            Language::JavaScript => JS_KIND_MAP,
            Language::Java => JAVA_KIND_MAP,
            Language::Kotlin => KOTLIN_KIND_MAP,
            Language::CSharp => CSHARP_KIND_MAP,
            Language::Php => PHP_KIND_MAP,
            Language::Ruby => RUBY_KIND_MAP,
            Language::Swift => SWIFT_KIND_MAP,
            Language::C => C_KIND_MAP,
            Language::Cpp => CPP_KIND_MAP,
            Language::Dart => DART_KIND_MAP,
        }
    }

    pub(super) fn kind_for_pattern(self, pattern_index: usize) -> SymbolKind {
        // Runtime kept permissive (test suite pins the exact mapping so drift
        // fails CI loud). See parse-hardening-tree-sitter design.md Decision 4
        // for why this does not panic on an unknown pattern index.
        self.kind_map()
            .get(pattern_index)
            .copied()
            .unwrap_or(SymbolKind::Function)
    }

    /// Tree-sitter query that captures callee names at call sites.
    ///
    /// Each match yields a `@callee` capture with the name of the called
    /// function or method. Phase-1 approximate resolution: only the local
    /// name is captured (not the full qualified path).
    pub(super) fn call_query(self) -> &'static str {
        match self {
            Language::Rust => RUST_CALL_QUERY,
            Language::Python => PYTHON_CALL_QUERY,
            Language::TypeScript | Language::Tsx => TS_CALL_QUERY,
            Language::Go => GO_CALL_QUERY,
            Language::JavaScript => JS_CALL_QUERY,
            Language::Java => JAVA_CALL_QUERY,
            Language::Kotlin => KOTLIN_CALL_QUERY,
            Language::CSharp => CSHARP_CALL_QUERY,
            Language::Php => PHP_CALL_QUERY,
            Language::Ruby => RUBY_CALL_QUERY,
            Language::Swift => SWIFT_CALL_QUERY,
            Language::C => C_CALL_QUERY,
            Language::Cpp => CPP_CALL_QUERY,
            Language::Dart => DART_CALL_QUERY,
        }
    }

    /// Tree-sitter query that captures import/use references.
    ///
    /// Each match yields an `@import_ref` capture with the module path or
    /// name being imported. Phase-1 approximate: the raw text is returned
    /// as-is for downstream resolution.
    pub(super) fn import_query(self) -> &'static str {
        match self {
            Language::Rust => RUST_IMPORT_QUERY,
            Language::Python => PYTHON_IMPORT_QUERY,
            Language::TypeScript | Language::Tsx => TS_IMPORT_QUERY,
            Language::Go => GO_IMPORT_QUERY,
            Language::JavaScript => JS_IMPORT_QUERY,
            Language::Java => JAVA_IMPORT_QUERY,
            Language::Kotlin => KOTLIN_IMPORT_QUERY,
            Language::CSharp => CSHARP_IMPORT_QUERY,
            Language::Php => PHP_IMPORT_QUERY,
            Language::Ruby => RUBY_IMPORT_QUERY,
            Language::Swift => SWIFT_IMPORT_QUERY,
            Language::C => C_IMPORT_QUERY,
            Language::Cpp => CPP_IMPORT_QUERY,
            Language::Dart => DART_IMPORT_QUERY,
        }
    }

    /// Call mode map: pattern index -> CallMode (Free or Method).
    ///
    /// Used by stage-4 resolution to determine whether a call site is a
    /// free function call or a method/attribute call for scoring purposes.
    pub(super) fn call_mode_map(self) -> &'static [super::CallMode] {
        match self {
            Language::Rust => RUST_CALL_MODE_MAP,
            Language::Python => PYTHON_CALL_MODE_MAP,
            Language::TypeScript | Language::Tsx => TS_CALL_MODE_MAP,
            Language::Go => GO_CALL_MODE_MAP,
            Language::JavaScript => JS_CALL_MODE_MAP,
            Language::Java => JAVA_CALL_MODE_MAP,
            Language::Kotlin => KOTLIN_CALL_MODE_MAP,
            Language::CSharp => CSHARP_CALL_MODE_MAP,
            Language::Php => PHP_CALL_MODE_MAP,
            Language::Ruby => RUBY_CALL_MODE_MAP,
            Language::Swift => SWIFT_CALL_MODE_MAP,
            Language::C => C_CALL_MODE_MAP,
            Language::Cpp => CPP_CALL_MODE_MAP,
            Language::Dart => DART_CALL_MODE_MAP,
        }
    }
}

include!("language/definitions.rs");
include!("language/calls_imports.rs");
