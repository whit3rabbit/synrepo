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
}

impl Language {
    /// Resolve a file extension to a `Language`, if supported.
    pub fn from_extension(ext: &str) -> Option<Language> {
        match ext {
            "rs" => Some(Language::Rust),
            "py" => Some(Language::Python),
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
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
        }
    }

    pub(super) fn definition_query(self) -> &'static str {
        match self {
            Language::Rust => RUST_DEFINITION_QUERY,
            Language::Python => PYTHON_DEFINITION_QUERY,
            Language::TypeScript | Language::Tsx => TS_DEFINITION_QUERY,
        }
    }

    pub(super) fn kind_for_pattern(self, pattern_index: usize) -> SymbolKind {
        match self {
            Language::Rust => RUST_KIND_MAP
                .get(pattern_index)
                .copied()
                .unwrap_or(SymbolKind::Function),
            Language::Python => PYTHON_KIND_MAP
                .get(pattern_index)
                .copied()
                .unwrap_or(SymbolKind::Function),
            Language::TypeScript | Language::Tsx => TS_KIND_MAP
                .get(pattern_index)
                .copied()
                .unwrap_or(SymbolKind::Function),
        }
    }
}

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

const PYTHON_DEFINITION_QUERY: &str = r#"
(function_definition name: (identifier) @name) @item
(class_definition name: (identifier) @name) @item
"#;

const PYTHON_KIND_MAP: &[SymbolKind] = &[SymbolKind::Function, SymbolKind::Class];

const TS_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(class_declaration name: (type_identifier) @name) @item
(interface_declaration name: (type_identifier) @name) @item
(type_alias_declaration name: (type_identifier) @name) @item
(method_definition name: (property_identifier) @name) @item
(abstract_method_signature name: (property_identifier) @name) @item
"#;

const TS_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Trait,
    SymbolKind::Type,
    SymbolKind::Method,
    SymbolKind::Method,
];
