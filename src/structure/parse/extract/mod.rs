mod docs;
mod qualname;

use std::path::Path;
use std::sync::OnceLock;

use tree_sitter::StreamingIterator as _;

use super::{ExtractedCallRef, ExtractedImportRef, ExtractedSymbol, Language, ParseOutput};

/// Cached definition query with capture indices for "item" and "name".
struct DefinitionQuery {
    query: Box<tree_sitter::Query>,
    item_idx: u32,
    name_idx: u32,
}

/// Cached single-capture query (call or import).
struct SingleCaptureQuery {
    query: Box<tree_sitter::Query>,
    capture_idx: u32,
}

// Single source of truth for "every language we compile queries for".
// Adding a variant to `Language` and listing it here is the only step
// needed to extend the cache; the three init fns below iterate this slice.
const LANGUAGES: &[Language] = &[
    Language::Rust,
    Language::Python,
    Language::TypeScript,
    Language::Tsx,
    Language::Go,
];

/// Global caches for compiled tree-sitter queries.
/// Each query is compiled once per language and reused across all file parses.
static DEFINITION_QUERIES: OnceLock<Vec<Option<DefinitionQuery>>> = OnceLock::new();
static CALL_QUERIES: OnceLock<Vec<Option<SingleCaptureQuery>>> = OnceLock::new();
static IMPORT_QUERIES: OnceLock<Vec<Option<SingleCaptureQuery>>> = OnceLock::new();

/// Initialize all definition queries for all languages.
fn init_definition_queries() -> Vec<Option<DefinitionQuery>> {
    let mut cache: Vec<Option<DefinitionQuery>> = (0..LANGUAGES.len()).map(|_| None).collect();
    for &lang in LANGUAGES {
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
fn init_call_queries() -> Vec<Option<SingleCaptureQuery>> {
    let mut cache: Vec<Option<SingleCaptureQuery>> = (0..LANGUAGES.len()).map(|_| None).collect();
    for &lang in LANGUAGES {
        let ts_lang = lang.tree_sitter_language();
        let query_text = lang.call_query();
        match tree_sitter::Query::new(&ts_lang, query_text) {
            Ok(query) => {
                let capture_names = query.capture_names();
                let capture_idx = capture_names
                    .iter()
                    .position(|n| *n == "callee")
                    .map(|i| i as u32);
                if let Some(capture_idx) = capture_idx {
                    cache[lang as usize] = Some(SingleCaptureQuery {
                        query: Box::new(query),
                        capture_idx,
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
fn init_import_queries() -> Vec<Option<SingleCaptureQuery>> {
    let mut cache: Vec<Option<SingleCaptureQuery>> = (0..LANGUAGES.len()).map(|_| None).collect();
    for &lang in LANGUAGES {
        let ts_lang = lang.tree_sitter_language();
        let query_text = lang.import_query();
        match tree_sitter::Query::new(&ts_lang, query_text) {
            Ok(query) => {
                let capture_names = query.capture_names();
                let capture_idx = capture_names
                    .iter()
                    .position(|n| *n == "import_ref")
                    .map(|i| i as u32);
                if let Some(capture_idx) = capture_idx {
                    cache[lang as usize] = Some(SingleCaptureQuery {
                        query: Box::new(query),
                        capture_idx,
                    });
                }
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
fn get_call_query(language: Language) -> Option<&'static SingleCaptureQuery> {
    CALL_QUERIES
        .get_or_init(init_call_queries)
        .get(language as usize)?
        .as_ref()
}

/// Get the cached import query for a language.
fn get_import_query(language: Language) -> Option<&'static SingleCaptureQuery> {
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

        symbols.push(ExtractedSymbol {
            qualified_name,
            display_name: name,
            kind,
            body_byte_range: body_range,
            body_hash: hex::encode(blake3::hash(body_bytes).as_bytes()),
            signature: docs::extract_signature(item_node, content, language),
            doc_comment: docs::extract_doc_comment(item_node, content, language),
        });
    }

    let call_refs = extract_call_refs(language, &tree, content)?;
    let import_refs = extract_import_refs(language, &tree, content)?;

    Ok(Some(ParseOutput {
        language,
        symbols,
        edges: vec![],
        call_refs,
        import_refs,
    }))
}

/// Extract call-site references from a parsed file for stage-4 resolution.
fn extract_call_refs(
    language: Language,
    tree: &tree_sitter::Tree,
    content: &[u8],
) -> crate::Result<Vec<ExtractedCallRef>> {
    // Use cached call query
    let Some(call_query) = get_call_query(language) else {
        return Ok(vec![]);
    };

    let callee_idx = call_query.capture_idx;
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&call_query.query, tree.root_node(), content);
    let mut refs = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures.iter().filter(|c| c.index == callee_idx) {
            let name = node_text(capture.node, content);
            if !name.is_empty() {
                refs.push(ExtractedCallRef { callee_name: name });
            }
        }
    }

    Ok(refs)
}

/// Extract import/use references from a parsed file for stage-4 resolution.
fn extract_import_refs(
    language: Language,
    tree: &tree_sitter::Tree,
    content: &[u8],
) -> crate::Result<Vec<ExtractedImportRef>> {
    // Use cached import query
    let Some(import_query) = get_import_query(language) else {
        return Ok(vec![]);
    };

    let ref_idx = import_query.capture_idx;
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&import_query.query, tree.root_node(), content);
    let mut refs = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures.iter().filter(|c| c.index == ref_idx) {
            let module_ref = node_text(capture.node, content);
            if !module_ref.is_empty() {
                refs.push(ExtractedImportRef { module_ref });
            }
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
