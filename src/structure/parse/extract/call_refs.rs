use tree_sitter::StreamingIterator as _;

use super::{get_call_query, node_text};
use crate::structure::{
    graph::SymbolKind,
    parse::{CallMode, ExtractedCallRef, ExtractedCallerSymbol, ExtractedSymbol, Language},
};

/// Extract call-site references from a parsed file for stage-4 resolution.
pub(super) fn extract_call_refs(
    language: Language,
    tree: &tree_sitter::Tree,
    content: &[u8],
    symbols: &[ExtractedSymbol],
) -> crate::Result<Vec<ExtractedCallRef>> {
    let Some(call_query) = get_call_query(language) else {
        return Ok(vec![]);
    };

    let callee_idx = call_query.callee_idx;
    let prefix_idx = call_query.prefix_idx;
    let call_mode_map = language.call_mode_map();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&call_query.query, tree.root_node(), content);
    let mut refs = Vec::new();

    while let Some(m) = matches.next() {
        let pattern_index = m.pattern_index;
        let is_method = call_mode_map
            .get(pattern_index)
            .map(|&mode| mode == CallMode::Method)
            .unwrap_or(false);

        for capture in m.captures.iter().filter(|c| c.index == callee_idx) {
            let name = node_text(capture.node, content);
            if name.is_empty() {
                continue;
            }

            let callee_prefix = prefix_idx.and_then(|idx| {
                m.captures
                    .iter()
                    .find(|c| c.index == idx)
                    .map(|c| node_text(c.node, content))
            });

            refs.push(ExtractedCallRef {
                callee_name: name,
                callee_prefix: callee_prefix.filter(|s| !s.is_empty()),
                is_method,
                caller: enclosing_caller(capture.node, symbols),
            });
        }
    }

    Ok(refs)
}

fn enclosing_caller(
    node: tree_sitter::Node<'_>,
    symbols: &[ExtractedSymbol],
) -> Option<ExtractedCallerSymbol> {
    let byte = node.start_byte() as u32;
    symbols
        .iter()
        .filter(|symbol| matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method))
        .filter(|symbol| symbol.body_byte_range.0 <= byte && byte <= symbol.body_byte_range.1)
        .min_by_key(|symbol| symbol.body_byte_range.1 - symbol.body_byte_range.0)
        .map(|symbol| ExtractedCallerSymbol {
            qualified_name: symbol.qualified_name.clone(),
            body_hash: symbol.body_hash.clone(),
        })
}
