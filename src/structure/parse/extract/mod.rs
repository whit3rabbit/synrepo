mod call_refs;
mod docs;
mod qualname;
mod visibility;

use std::path::Path;
use std::sync::OnceLock;

use tree_sitter::StreamingIterator as _;

use super::{ExtractedImportRef, ExtractedSymbol, Language, ParseOutput};

/// Cached definition query with capture indices for "item" and "name".
struct DefinitionQuery {
    query: Box<tree_sitter::Query>,
    item_idx: u32,
    name_idx: u32,
}

/// Cached call query with capture indices for @callee and optional @callee_prefix.
struct CallQuery {
    query: Box<tree_sitter::Query>,
    callee_idx: u32,
    /// Optional capture index for @callee_prefix (method/qualified calls).
    prefix_idx: Option<u32>,
}

/// Cached import query with optional fan-out capture indices.
///
/// Most patterns expose a single `@import_ref` (module path). A few
/// patterns fan out: Rust braced-use binds `@use_path` plus `@use_item`
/// per leaf; Python `from foo import bar` binds `@import_ref` plus
/// `@import_name`. The extractor joins those with the language
/// separator so the resolver sees one flat `ExtractedImportRef` per
/// leaf.
struct ImportQuery {
    query: Box<tree_sitter::Query>,
    import_ref_idx: Option<u32>,
    use_path_idx: Option<u32>,
    use_item_idx: Option<u32>,
    import_name_idx: Option<u32>,
}

// Single source of truth lives on `Language::supported()`. The query cache
// sizes itself to the enum's discriminant range (5 variants today) so
// `cache[lang as usize]` remains a direct index. Adding a `Language` variant
// requires updating `Language::supported()` and extending each per-language
// `match` arm in `language.rs`; validation tests iterate `supported()` and
// fail CI if a new variant is missing coverage.
fn supported_languages() -> &'static [Language] {
    Language::supported()
}

/// Global caches for compiled tree-sitter queries.
/// Each query is compiled once per language and reused across all file parses.
static DEFINITION_QUERIES: OnceLock<Vec<Option<DefinitionQuery>>> = OnceLock::new();
static CALL_QUERIES: OnceLock<Vec<Option<CallQuery>>> = OnceLock::new();
static IMPORT_QUERIES: OnceLock<Vec<Option<ImportQuery>>> = OnceLock::new();

/// Initialize all definition queries for all languages.
fn init_definition_queries() -> Vec<Option<DefinitionQuery>> {
    let languages = supported_languages();
    let mut cache: Vec<Option<DefinitionQuery>> = (0..languages.len()).map(|_| None).collect();
    for &lang in languages {
        let ts_lang = lang.tree_sitter_language();
        let query_text = lang.definition_query();
        match tree_sitter::Query::new(&ts_lang, query_text) {
            Ok(query) => {
                let capture_names = query.capture_names();
                let item_idx = capture_names
                    .iter()
                    .position(|n| *n == "item")
                    .map(|i| i as u32);
                let name_idx = capture_names
                    .iter()
                    .position(|n| *n == "name")
                    .map(|i| i as u32);
                if let (Some(item_idx), Some(name_idx)) = (item_idx, name_idx) {
                    cache[lang as usize] = Some(DefinitionQuery {
                        query: Box::new(query),
                        item_idx,
                        name_idx,
                    });
                }
            }
            Err(e) => {
                // Query compilation failure - leave as None, callers will handle.
                tracing::warn!(?lang, error = %e, "failed to compile definition query");
            }
        }
    }
    cache
}

/// Initialize all call queries for all languages.
fn init_call_queries() -> Vec<Option<CallQuery>> {
    let languages = supported_languages();
    let mut cache: Vec<Option<CallQuery>> = (0..languages.len()).map(|_| None).collect();
    for &lang in languages {
        let ts_lang = lang.tree_sitter_language();
        let query_text = lang.call_query();
        match tree_sitter::Query::new(&ts_lang, query_text) {
            Ok(query) => {
                let capture_names = query.capture_names();
                let callee_idx = capture_names
                    .iter()
                    .position(|n| *n == "callee")
                    .map(|i| i as u32);
                let prefix_idx = capture_names
                    .iter()
                    .position(|n| *n == "callee_prefix")
                    .map(|i| i as u32);
                if let Some(callee_idx) = callee_idx {
                    cache[lang as usize] = Some(CallQuery {
                        query: Box::new(query),
                        callee_idx,
                        prefix_idx,
                    });
                }
            }
            Err(e) => {
                tracing::warn!(?lang, error = %e, "failed to compile call query");
            }
        }
    }
    cache
}

/// Initialize all import queries for all languages.
fn init_import_queries() -> Vec<Option<ImportQuery>> {
    let languages = supported_languages();
    let mut cache: Vec<Option<ImportQuery>> = (0..languages.len()).map(|_| None).collect();
    for &lang in languages {
        let ts_lang = lang.tree_sitter_language();
        let query_text = lang.import_query();
        match tree_sitter::Query::new(&ts_lang, query_text) {
            Ok(query) => {
                let find = |name: &str| -> Option<u32> {
                    query
                        .capture_names()
                        .iter()
                        .position(|n| *n == name)
                        .map(|i| i as u32)
                };
                let import_ref_idx = find("import_ref");
                let use_path_idx = find("use_path");
                let use_item_idx = find("use_item");
                let import_name_idx = find("import_name");
                cache[lang as usize] = Some(ImportQuery {
                    query: Box::new(query),
                    import_ref_idx,
                    use_path_idx,
                    use_item_idx,
                    import_name_idx,
                });
            }
            Err(e) => {
                tracing::warn!(?lang, error = %e, "failed to compile import query");
            }
        }
    }
    cache
}

/// Get the cached definition query for a language.
fn get_definition_query(language: Language) -> Option<&'static DefinitionQuery> {
    DEFINITION_QUERIES
        .get_or_init(init_definition_queries)
        .get(language as usize)?
        .as_ref()
}

/// Get the cached call query for a language.
fn get_call_query(language: Language) -> Option<&'static CallQuery> {
    CALL_QUERIES
        .get_or_init(init_call_queries)
        .get(language as usize)?
        .as_ref()
}

/// Get the cached import query for a language.
fn get_import_query(language: Language) -> Option<&'static ImportQuery> {
    IMPORT_QUERIES
        .get_or_init(init_import_queries)
        .get(language as usize)?
        .as_ref()
}

/// Parse a source file and extract symbols and within-file edges.
///
/// Returns `None` if the file extension is not supported by any wired grammar.
/// Returns `Some(ParseOutput)` (possibly with empty symbol list) otherwise.
/// Parse errors inside tree-sitter are treated as partial results rather than
/// hard failures, because syntax errors in the source file should not prevent
/// the rest of the graph from being populated.
pub fn parse_file(path: &Path, content: &[u8]) -> crate::Result<Option<ParseOutput>> {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return Ok(None);
    };
    let Some(language) = Language::from_extension(ext) else {
        return Ok(None);
    };

    let ts_language = language.tree_sitter_language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_language)
        .map_err(|error| crate::Error::Parse {
            path: path.display().to_string(),
            message: format!("failed to set language: {error}"),
        })?;

    let Some(tree) = parser.parse(content, None) else {
        return Ok(Some(ParseOutput {
            language,
            symbols: vec![],
            edges: vec![],
            call_refs: vec![],
            import_refs: vec![],
        }));
    };

    // Use cached definition query instead of compiling per-file
    let Some(def_query) = get_definition_query(language) else {
        return Ok(Some(ParseOutput {
            language,
            symbols: vec![],
            edges: vec![],
            call_refs: vec![],
            import_refs: vec![],
        }));
    };

    let (item_idx, name_idx) = (def_query.item_idx, def_query.name_idx);

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut cursor_matches = cursor.matches(&def_query.query, tree.root_node(), content);
    let mut symbols = Vec::new();

    while let Some(query_match) = cursor_matches.next() {
        let item_node = query_match
            .captures
            .iter()
            .find(|capture| capture.index == item_idx)
            .map(|capture| capture.node);
        let name_node = query_match
            .captures
            .iter()
            .find(|capture| capture.index == name_idx)
            .map(|capture| capture.node);

        let (Some(item_node), Some(name_node)) = (item_node, name_node) else {
            continue;
        };

        let name = node_text(name_node, content);
        if name.is_empty() {
            continue;
        }

        let base_kind = language.kind_for_pattern(query_match.pattern_index);
        let (qualified_name, kind) =
            qualname::build_qualified_name_and_kind(item_node, &name, content, base_kind);

        let body_range = (item_node.start_byte() as u32, item_node.end_byte() as u32);
        let body_bytes = &content[item_node.start_byte()..item_node.end_byte()];

        // Extract visibility using name reference before moving name.
        let symbol_visibility = visibility::extract_visibility(item_node, content, language, &name);

        symbols.push(ExtractedSymbol {
            qualified_name,
            display_name: name,
            kind,
            visibility: symbol_visibility,
            body_byte_range: body_range,
            body_hash: hex::encode(blake3::hash(body_bytes).as_bytes()),
            signature: docs::extract_signature(item_node, content, language),
            doc_comment: docs::extract_doc_comment(item_node, content, language),
        });
    }

    let call_refs = call_refs::extract_call_refs(language, &tree, content, &symbols)?;
    let import_refs = extract_import_refs(language, &tree, content)?;

    Ok(Some(ParseOutput {
        language,
        symbols,
        edges: vec![],
        call_refs,
        import_refs,
    }))
}

/// Extract import/use references from a parsed file for stage-4 resolution.
///
/// Patterns fall into two shapes:
/// * Single-capture `@import_ref` — take the node's text verbatim.
/// * Dual-capture fan-out — `@use_path` + `@use_item` (Rust braced-use)
///   joined with `::`, or `@import_ref` + `@import_name` (Python
///   from-import) joined with `.`. One leaf per match; the query
///   engine fires one match per list entry.
fn extract_import_refs(
    language: Language,
    tree: &tree_sitter::Tree,
    content: &[u8],
) -> crate::Result<Vec<ExtractedImportRef>> {
    let Some(import_query) = get_import_query(language) else {
        return Ok(vec![]);
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&import_query.query, tree.root_node(), content);
    let mut refs = Vec::new();

    while let Some(m) = matches.next() {
        let find_text = |idx: Option<u32>| -> Option<String> {
            let idx = idx?;
            let capture = m.captures.iter().find(|c| c.index == idx)?;
            let text = node_text(capture.node, content);
            (!text.is_empty()).then_some(text)
        };

        // Rust braced-use fan-out: `use foo::{a};` -> "foo::a".
        if let (Some(path), Some(item)) = (
            find_text(import_query.use_path_idx),
            find_text(import_query.use_item_idx),
        ) {
            refs.push(ExtractedImportRef {
                module_ref: format!("{path}::{item}"),
            });
            continue;
        }

        // Python from-import fan-out: `from foo import bar` -> "foo.bar".
        // Emitted alongside the bare "foo" produced by the single-capture
        // pattern; the resolver dedupes via `HashSet<FileNodeId>`.
        if let (Some(module), Some(name)) = (
            find_text(import_query.import_ref_idx),
            find_text(import_query.import_name_idx),
        ) {
            refs.push(ExtractedImportRef {
                module_ref: format!("{module}.{name}"),
            });
            continue;
        }

        // Single-capture fallback (covers Rust bare/scoped_identifier, Python
        // `import foo`, TS `import ... from './x'`, TS re-exports, Go).
        if let Some(module_ref) = find_text(import_query.import_ref_idx) {
            refs.push(ExtractedImportRef { module_ref });
        }
    }

    Ok(refs)
}

/// Extract the raw text of a tree-sitter node from the source buffer.
fn node_text(node: tree_sitter::Node, source: &[u8]) -> String {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
        .unwrap_or("")
        .to_string()
}
