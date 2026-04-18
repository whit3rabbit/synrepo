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
        }
    }

    pub(super) fn definition_query(self) -> &'static str {
        match self {
            Language::Rust => RUST_DEFINITION_QUERY,
            Language::Python => PYTHON_DEFINITION_QUERY,
            Language::TypeScript | Language::Tsx => TS_DEFINITION_QUERY,
            Language::Go => GO_DEFINITION_QUERY,
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

const RUST_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (field_expression field: (field_identifier) @callee))
(call_expression function: (scoped_identifier name: (identifier) @callee))
"#;

const PYTHON_CALL_QUERY: &str = r#"
(call function: (identifier) @callee)
(call function: (attribute attribute: (identifier) @callee))
"#;

const TS_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (member_expression property: (property_identifier) @callee))
"#;

// --- Import/use queries (stage 4: cross-file edge resolution) ---

// Captures the full argument text of a `use_declaration`. The
// `scoped_identifier` node's text is the whole `::`-separated path
// (e.g., `std::collections::HashMap`, `crate::util::helper`), which
// stage 4's Rust resolver needs to map onto candidate module files.
// The bare-identifier arm still covers single-segment `use foo;`.
const RUST_IMPORT_QUERY: &str = r#"
(use_declaration argument: (identifier) @import_ref)
(use_declaration argument: (scoped_identifier) @import_ref)
"#;

const PYTHON_IMPORT_QUERY: &str = r#"
(import_statement name: (dotted_name) @import_ref)
(import_from_statement module_name: (dotted_name) @import_ref)
"#;

const TS_IMPORT_QUERY: &str = r#"
(import_statement source: (string (string_fragment) @import_ref))
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

const GO_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (selector_expression field: (field_identifier) @callee))
"#;

const GO_IMPORT_QUERY: &str = r#"
(import_spec path: (interpreted_string_literal) @import_ref)
"#;
