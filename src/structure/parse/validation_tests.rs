//! Parser-invariant validation suite.
//!
//! These tests pin the tree-sitter query contract (compile cleanly, expose
//! the capture names extraction depends on) and the pattern-index to
//! `SymbolKind` mapping that `Language::kind_for_pattern` reads at runtime.
//! The runtime fallback to `SymbolKind::Function` stays in place for
//! forward-compatibility with untested grammar patterns; the tests below
//! assert every pattern index a supported language emits has an explicit
//! slot, so drift fails CI loud instead of degrading silently.

use super::Language;
use crate::{
    core::source_language::SOURCE_LANGUAGE_SPECS,
    structure::graph::SymbolKind,
    substrate::{classify as classify_file, FileClass},
};
use std::path::Path;

/// Role of the query inside a single-language validation check, used for
/// diagnostics so a failure identifies both the language and which query
/// broke.
#[derive(Clone, Copy)]
enum QueryRole {
    Definition,
    Call,
    Import,
}

impl QueryRole {
    fn as_str(self) -> &'static str {
        match self {
            QueryRole::Definition => "definition",
            QueryRole::Call => "call",
            QueryRole::Import => "import",
        }
    }
}

fn compile_query(lang: Language, role: QueryRole) -> tree_sitter::Query {
    let ts_lang = lang.tree_sitter_language();
    let text = match role {
        QueryRole::Definition => lang.definition_query(),
        QueryRole::Call => lang.call_query(),
        QueryRole::Import => lang.import_query(),
    };
    tree_sitter::Query::new(&ts_lang, text).unwrap_or_else(|error| {
        panic!(
            "{role} query for {lang} failed to compile: {error}",
            role = role.as_str(),
            lang = lang.display_name(),
        )
    })
}

fn assert_capture_present(
    query: &tree_sitter::Query,
    capture: &str,
    lang: Language,
    role: QueryRole,
) {
    let names = query.capture_names();
    assert!(
        names.iter().any(|n| *n == capture),
        "{role} query for {lang} is missing required capture @{capture} (found: {names:?})",
        role = role.as_str(),
        lang = lang.display_name(),
    );
}

// ── Task 1: query compile + capture contract ─────────────────────────────────

/// 1.2: every supported `Language` has a definition query that compiles
/// and exposes `@item` and `@name`.
#[test]
fn definition_queries_compile_and_expose_item_and_name() {
    for &lang in Language::supported() {
        let query = compile_query(lang, QueryRole::Definition);
        assert_capture_present(&query, "item", lang, QueryRole::Definition);
        assert_capture_present(&query, "name", lang, QueryRole::Definition);
    }
}

/// 1.3: every supported `Language` has a call query that compiles and
/// exposes `@callee` (call extraction is wired for every language today;
/// if a future variant opts out, add an explicit allowlist here rather
/// than weakening this assertion).
#[test]
fn call_queries_compile_and_expose_callee() {
    for &lang in Language::supported() {
        let query = compile_query(lang, QueryRole::Call);
        assert_capture_present(&query, "callee", lang, QueryRole::Call);
    }
}

/// 1.4: every supported `Language` has an import query that compiles and
/// exposes `@import_ref`.
#[test]
fn import_queries_compile_and_expose_import_ref() {
    for &lang in Language::supported() {
        let query = compile_query(lang, QueryRole::Import);
        assert_capture_present(&query, "import_ref", lang, QueryRole::Import);
    }
}

#[test]
fn classifier_and_parser_share_supported_language_registry() {
    for spec in SOURCE_LANGUAGE_SPECS {
        for ext in spec.extensions {
            let path = format!("fixture.{ext}");
            assert_eq!(
                classify_file(Path::new(&path), 12, b"void main() {}"),
                FileClass::SupportedCode {
                    language: spec.label
                },
                "{path} must classify as supported code"
            );
            assert!(
                Language::from_extension(ext).is_some(),
                "{path} must also resolve to a parser language"
            );
        }
    }
}

// ── Task 2: pattern-index → SymbolKind mapping is pinned ─────────────────────

/// 2.1 / 2.2: the kind map's length equals the compiled definition query's
/// pattern_count for every language, so every pattern index the query
/// emits has an explicit `SymbolKind` slot. No reliance on the runtime
/// `SymbolKind::Function` fallback to pass.
#[test]
fn kind_map_covers_every_definition_query_pattern() {
    for &lang in Language::supported() {
        let query = compile_query(lang, QueryRole::Definition);
        let map = lang.kind_map();
        assert_eq!(
            map.len(),
            query.pattern_count(),
            "{lang}: kind_map len ({map_len}) does not match definition_query pattern_count ({qp}). \
             Either the query gained/lost a pattern or the kind table was not updated.",
            lang = lang.display_name(),
            map_len = map.len(),
            qp = query.pattern_count(),
        );
    }
}

/// 2.1 (cont.): pin the exact per-language mapping so reordering patterns
/// without touching the table fails immediately. Pattern order drives
/// `kind_for_pattern`, which stage 3 calls for every matched item — a
/// silent reorder would mis-kind every symbol in the affected file.
#[test]
fn rust_kind_map_is_pinned() {
    use SymbolKind::*;
    assert_eq!(
        Language::Rust.kind_map(),
        &[
            Function, // function_item
            Class,    // struct_item
            Class,    // enum_item
            Trait,    // trait_item
            Type,     // type_item
            Module,   // mod_item
            Constant, // const_item
            Constant, // static_item
        ]
    );
}

#[test]
fn python_kind_map_is_pinned() {
    use SymbolKind::*;
    assert_eq!(
        Language::Python.kind_map(),
        &[
            Function, // function_definition
            Class,    // class_definition
        ]
    );
}

#[test]
fn typescript_kind_map_is_pinned() {
    use SymbolKind::*;
    let expected: &[SymbolKind] = &[
        Function, // function_declaration
        Class,    // class_declaration
        Trait,    // interface_declaration
        Type,     // type_alias_declaration
        Method,   // method_definition
        Method,   // abstract_method_signature
        Class,    // variable_declarator → (class)
    ];
    assert_eq!(Language::TypeScript.kind_map(), expected);
    // TSX shares the same query body + kind map by design.
    assert_eq!(Language::Tsx.kind_map(), expected);
}

#[test]
fn go_kind_map_is_pinned() {
    use SymbolKind::*;
    assert_eq!(
        Language::Go.kind_map(),
        &[
            Function,  // function_declaration
            Method,    // method_declaration
            Interface, // type_spec (interface_type)
            Class,     // type_spec (struct_type)
            Constant,  // const_spec
            Constant,  // var_spec
        ]
    );
}

// ── Task 3: call-mode map coverage ──────────────────────────────────────────

/// The call-mode map's length must equal the compiled call query's pattern
/// count for every language, so every pattern index the query emits has an
/// explicit `CallMode` slot.
#[test]
fn call_mode_map_covers_every_call_query_pattern() {
    for &lang in Language::supported() {
        let query = compile_query(lang, QueryRole::Call);
        let map = lang.call_mode_map();
        assert_eq!(
            map.len(),
            query.pattern_count(),
            "{lang}: call_mode_map len ({map_len}) does not match call_query pattern_count ({qp}). \
             Either the query gained/lost a pattern or the mode table was not updated.",
            lang = lang.display_name(),
            map_len = map.len(),
            qp = query.pattern_count(),
        );
    }
}
