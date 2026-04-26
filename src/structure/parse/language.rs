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
        match ext {
            "rs" => Some(Language::Rust),
            "py" => Some(Language::Python),
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "go" => Some(Language::Go),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "java" => Some(Language::Java),
            "kt" | "kts" => Some(Language::Kotlin),
            "cs" => Some(Language::CSharp),
            "php" => Some(Language::Php),
            "rb" => Some(Language::Ruby),
            "swift" => Some(Language::Swift),
            "c" | "h" => Some(Language::C),
            "cpp" | "hpp" | "cc" | "cxx" => Some(Language::Cpp),
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

    /// Return the fixed pattern-index → `SymbolKind` table for this language.
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

    /// Call mode map: pattern index → CallMode (Free or Method).
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

// Pattern index → kind (see RUST_KIND_MAP):
//   0: function_item → Function
//   1: struct_item   → Class
//   2: enum_item     → Class
//   3: trait_item    → Trait
//   4: type_item     → Type
//   5: mod_item      → Module
//   6: const_item    → Constant
//   7: static_item   → Constant
const RUST_DEFINITION_QUERY: &str = r#"
(function_item name: (identifier) @name) @item
(struct_item name: (type_identifier) @name) @item
(enum_item name: (type_identifier) @name) @item
(trait_item name: (type_identifier) @name) @item
(type_item name: (type_identifier) @name) @item
(mod_item name: (identifier) @name) @item
(const_item name: (identifier) @name) @item
(static_item name: (identifier) @name) @item
"#;

const RUST_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Trait,
    SymbolKind::Type,
    SymbolKind::Module,
    SymbolKind::Constant,
    SymbolKind::Constant,
];

// Pattern index → kind (see PYTHON_KIND_MAP):
//   0: function_definition → Function
//   1: class_definition    → Class
const PYTHON_DEFINITION_QUERY: &str = r#"
(function_definition name: (identifier) @name) @item
(class_definition name: (identifier) @name) @item
"#;

const PYTHON_KIND_MAP: &[SymbolKind] = &[SymbolKind::Function, SymbolKind::Class];

// Pattern index → kind (see TS_KIND_MAP):
//   0: function_declaration              → Function
//   1: class_declaration                 → Class
//   2: interface_declaration             → Trait
//   3: type_alias_declaration            → Type
//   4: method_definition                 → Method
//   5: abstract_method_signature         → Method
//   6: variable_declarator → (class)     → Class   (class-expression bound to a name)
const TS_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(class_declaration name: (type_identifier) @name) @item
(interface_declaration name: (type_identifier) @name) @item
(type_alias_declaration name: (type_identifier) @name) @item
(method_definition name: (property_identifier) @name) @item
(abstract_method_signature name: (property_identifier) @name) @item
(variable_declarator name: (identifier) @name value: (class) @item)
"#;

const TS_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Trait,
    SymbolKind::Type,
    SymbolKind::Method,
    SymbolKind::Method,
    SymbolKind::Class,
];

// --- Call-site queries (stage 4: cross-file edge resolution) ---

// Rust call patterns:
//   0: (call_expression function: (identifier) @callee) → Free function call
//   1: (call_expression function: (field_expression value: (_) @callee_prefix field: (field_identifier) @callee)) → Method call
//   2: (call_expression function: (scoped_identifier path: (_) @callee_prefix name: (identifier) @callee)) → Qualified call
const RUST_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (field_expression value: (_) @callee_prefix field: (field_identifier) @callee))
(call_expression function: (scoped_identifier path: (_) @callee_prefix name: (identifier) @callee))
"#;

// Pattern index → CallMode (see RUST_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
//   2: Qualified call (has prefix)
const RUST_CALL_MODE_MAP: &[super::CallMode] = &[
    super::CallMode::Free,
    super::CallMode::Method,
    super::CallMode::Method,
];

// Python call patterns:
//   0: (call function: (identifier) @callee) → Free function call
//   1: (call function: (attribute object: (_) @callee_prefix attribute: (identifier) @callee)) → Method call
const PYTHON_CALL_QUERY: &str = r#"
(call function: (identifier) @callee)
(call function: (attribute object: (_) @callee_prefix attribute: (identifier) @callee))
"#;

// Pattern index → CallMode (see PYTHON_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
const PYTHON_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];

// TypeScript call patterns:
//   0: (call_expression function: (identifier) @callee) → Free function call
//   1: (call_expression function: (member_expression object: (_) @callee_prefix property: (property_identifier) @callee)) → Method call
const TS_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (member_expression object: (_) @callee_prefix property: (property_identifier) @callee))
"#;

// Pattern index → CallMode (see TS_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
const TS_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];

// --- Import/use queries (stage 4: cross-file edge resolution) ---

// Captures the full argument text of a `use_declaration`. The
// `scoped_identifier` node's text is the whole `::`-separated path
// (e.g., `std::collections::HashMap`, `crate::util::helper`), which
// stage 4's Rust resolver needs to map onto candidate module files.
// The bare-identifier arm still covers single-segment `use foo;`.
//
// Braced `use foo::{a, b};` fans out: one match per leaf, each emitting
// `@use_path` (the scoped prefix) plus `@use_item` (the leaf). The
// extractor joins them with `::` so the resolver sees the same shape as
// a bare `scoped_identifier` capture.
const RUST_IMPORT_QUERY: &str = r#"
(use_declaration argument: (identifier) @import_ref)
(use_declaration argument: (scoped_identifier) @import_ref)
(use_declaration argument: (scoped_use_list path: (_) @use_path list: (use_list (identifier) @use_item)))
"#;

// Python `from foo import bar` also captures `@import_name` so the
// extractor emits `foo.bar` alongside the bare `foo` module. The
// resolver tolerates unresolved paths; the dotted leaf exists so that
// a future stage-5 pass can resolve to a specific symbol.
const PYTHON_IMPORT_QUERY: &str = r#"
(import_statement name: (dotted_name) @import_ref)
(import_from_statement module_name: (dotted_name) @import_ref)
(import_from_statement module_name: (dotted_name) @import_ref name: (dotted_name) @import_name)
"#;

// `export { foo } from './bar'` is a re-export; the `source` shape is
// identical to an `import_statement`, so the resolver needs no change.
const TS_IMPORT_QUERY: &str = r#"
(import_statement source: (string (string_fragment) @import_ref))
(export_statement source: (string (string_fragment) @import_ref))
"#;

// --- Go queries ---

// Pattern index → kind (see GO_KIND_MAP):
//   0: function_declaration → Function
//   1: method_declaration   → Method
//   2: interface type_spec  → Interface
//   3: struct type_spec     → Class
//   4: const_spec           → Constant
//   5: var_spec             → Constant
const GO_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(method_declaration name: (field_identifier) @name) @item
(type_spec name: (type_identifier) @name type: (interface_type)) @item
(type_spec name: (type_identifier) @name type: (struct_type)) @item
(const_spec name: (identifier) @name) @item
(var_spec name: (identifier) @name) @item
"#;

const GO_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Method,
    SymbolKind::Interface,
    SymbolKind::Class,
    SymbolKind::Constant,
    SymbolKind::Constant,
];

// Go call patterns:
//   0: (call_expression function: (identifier) @callee) → Free function call
//   1: (call_expression function: (selector_expression operand: (_) @callee_prefix field: (field_identifier) @callee)) → Method call
const GO_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (selector_expression operand: (_) @callee_prefix field: (field_identifier) @callee))
"#;

// Pattern index → CallMode (see GO_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
const GO_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];

const GO_IMPORT_QUERY: &str = r#"
(import_spec path: (interpreted_string_literal) @import_ref)
"#;


// --- JavaScript queries ---
const JS_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(method_definition name: (property_identifier) @name) @item
(variable_declarator name: (identifier) @name value: (arrow_function)) @item
"#;
const JS_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Method,
    SymbolKind::Function,
];
const JS_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (member_expression object: (_) @callee_prefix property: (property_identifier) @callee))
"#;
const JS_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const JS_IMPORT_QUERY: &str = r#"
(import_statement source: (string (string_fragment) @import_ref))
(export_statement source: (string (string_fragment) @import_ref))
"#;

// --- Java queries ---
const JAVA_DEFINITION_QUERY: &str = r#"
(method_declaration name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(interface_declaration name: (identifier) @name) @item
(enum_declaration name: (identifier) @name) @item
(annotation_type_declaration name: (identifier) @name) @item
"#;
const JAVA_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Class,
    SymbolKind::Type,
];
const JAVA_CALL_QUERY: &str = r#"
(method_invocation name: (identifier) @callee)
(method_invocation object: (_) @callee_prefix name: (identifier) @callee)
"#;
const JAVA_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const JAVA_IMPORT_QUERY: &str = r#"
(import_declaration (scoped_identifier) @import_ref)
"#;

// --- Kotlin queries ---
const KOTLIN_DEFINITION_QUERY: &str = r#"
(class_declaration name: (identifier) @name) @item
(function_declaration name: (identifier) @name) @item
(object_declaration name: (identifier) @name) @item
"#;
const KOTLIN_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Class,
    SymbolKind::Function,
    SymbolKind::Class,
];
const KOTLIN_CALL_QUERY: &str = r#"
(call_expression (identifier) @callee)
"#;
const KOTLIN_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const KOTLIN_IMPORT_QUERY: &str = r#"
(import (identifier) @import_ref)
"#;

// --- C# queries ---
const CSHARP_DEFINITION_QUERY: &str = r#"
(method_declaration name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(interface_declaration name: (identifier) @name) @item
(struct_declaration name: (identifier) @name) @item
(enum_declaration name: (identifier) @name) @item
(delegate_declaration name: (identifier) @name) @item
"#;
const CSHARP_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Type,
];
const CSHARP_CALL_QUERY: &str = r#"
(invocation_expression function: (identifier) @callee)
(invocation_expression function: (member_access_expression name: (identifier) @callee))
"#;
const CSHARP_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const CSHARP_IMPORT_QUERY: &str = r#"
(using_directive (identifier) @import_ref)
"#;

// --- PHP queries ---
const PHP_DEFINITION_QUERY: &str = r#"
(function_definition name: (name) @name) @item
(class_declaration name: (name) @name) @item
(interface_declaration name: (name) @name) @item
(trait_declaration name: (name) @name) @item
(method_declaration name: (name) @name) @item
"#;
const PHP_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Trait,
    SymbolKind::Method,
];
const PHP_CALL_QUERY: &str = r#"
(function_call_expression function: (name) @callee)
(member_call_expression name: (name) @callee)
"#;
const PHP_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const PHP_IMPORT_QUERY: &str = r#"
(namespace_use_clause (name) @import_ref)
"#;

// --- Ruby queries ---
const RUBY_DEFINITION_QUERY: &str = r#"
(method name: (identifier) @name) @item
(singleton_method name: (identifier) @name) @item
(class name: (constant) @name) @item
(module name: (constant) @name) @item
"#;
const RUBY_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Method,
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Module,
];
const RUBY_CALL_QUERY: &str = r#"
(call method: (identifier) @callee)
"#;
const RUBY_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free];
const RUBY_IMPORT_QUERY: &str = r#"
(call method: (identifier) @method_name (#eq? @method_name "require") arguments: (argument_list (string (string_content) @import_ref)))
"#;

// --- Swift queries ---
const SWIFT_DEFINITION_QUERY: &str = r#"
(function_declaration name: (simple_identifier) @name) @item
(class_declaration name: (type_identifier) @name) @item
(protocol_declaration name: (type_identifier) @name) @item
"#;
const SWIFT_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Interface,
];
const SWIFT_CALL_QUERY: &str = r#"
(call_expression (identifier) @callee)
(call_expression (navigation_expression suffix: (simple_identifier) @callee))
"#;
const SWIFT_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const SWIFT_IMPORT_QUERY: &str = r#"
(import_declaration (identifier) @import_ref)
"#;

// --- C queries ---
const C_DEFINITION_QUERY: &str = r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @item
(struct_specifier name: (type_identifier) @name) @item
(enum_specifier name: (type_identifier) @name) @item
"#;
const C_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Class,
];
const C_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
"#;
const C_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free];
const C_IMPORT_QUERY: &str = r#"
(preproc_include path: (system_lib_string) @import_ref)
(preproc_include path: (string_literal) @import_ref)
"#;

// --- C++ queries ---
const CPP_DEFINITION_QUERY: &str = r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @item
(function_definition declarator: (function_declarator declarator: (field_identifier) @name)) @item
(class_specifier name: (type_identifier) @name) @item
(struct_specifier name: (type_identifier) @name) @item
(enum_specifier name: (type_identifier) @name) @item
"#;
const CPP_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Class,
];
const CPP_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (field_expression field: (field_identifier) @callee))
"#;
const CPP_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const CPP_IMPORT_QUERY: &str = r#"
(preproc_include path: (system_lib_string) @import_ref)
(preproc_include path: (string_literal) @import_ref)
"#;

// --- Dart queries ---
const DART_DEFINITION_QUERY: &str = r#"
(function_signature name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(enum_declaration name: (identifier) @name) @item
(mixin_declaration name: (identifier) @name) @item
(extension_declaration name: (identifier) @name) @item
"#;
const DART_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Trait,
    SymbolKind::Class,
];
const DART_CALL_QUERY: &str = r#"
(identifier) @callee
"#;
const DART_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free];
const DART_IMPORT_QUERY: &str = r#"
(uri) @import_ref
"#;

