//! Optional graph context blocks for commentary prompts.

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::ids::NodeId;
use crate::structure::graph::{EdgeKind, GraphReader, SymbolKind, Visibility};

use super::{truncate_chars, CommentaryContextTarget};

const MAX_RELATED_FILES: usize = 5;
const MAX_RELATED_CHARS: usize = 2_000;
const MAX_MODULE_FILES: usize = 8;
const MAX_TREE_ENTRIES: usize = 80;
const MAX_ASSOCIATED_TESTS: usize = 10;
const MAX_MARKERS: usize = 12;
const MAX_NEIGHBORS: usize = 20;
const MAX_EXPORTED_SYMBOLS: usize = 20;

pub(super) fn optional_blocks(
    repo_root: &Path,
    graph: &dyn GraphReader,
    target: &CommentaryContextTarget,
) -> Vec<String> {
    let mut blocks = Vec::new();
    push_if_some(&mut blocks, import_block(graph, target));
    push_if_some(&mut blocks, call_block(graph, target.node_id()));
    push_if_some(&mut blocks, exported_symbols_block(graph, target));
    push_if_some(
        &mut blocks,
        associated_tests_block(graph, &target.file.path),
    );
    push_if_some(
        &mut blocks,
        governing_decisions_block(graph, target.node_id()),
    );
    push_if_some(&mut blocks, co_change_block(graph, target));
    if let Some(source) = read_limited(repo_root, &target.file.path, super::MAX_SOURCE_CHARS) {
        push_if_some(&mut blocks, open_markers_block(&source));
    }
    push_if_some(
        &mut blocks,
        dependency_sources_block(repo_root, graph, target),
    );
    if is_module_root(&target.file.path) {
        push_if_some(&mut blocks, module_tree_block(repo_root, &target.file.path));
        push_if_some(
            &mut blocks,
            module_peer_sources_block(repo_root, &target.file.path),
        );
    }
    blocks
}

fn push_if_some(blocks: &mut Vec<String>, block: Option<String>) {
    if let Some(block) = block {
        blocks.push(block);
    }
}

fn import_block(graph: &dyn GraphReader, target: &CommentaryContextTarget) -> Option<String> {
    let file_node = NodeId::File(target.file.id);
    let outbound = graph
        .outbound(file_node, Some(EdgeKind::Imports))
        .unwrap_or_default();
    let inbound = graph
        .inbound(file_node, Some(EdgeKind::Imports))
        .unwrap_or_default();
    if outbound.is_empty() && inbound.is_empty() {
        return None;
    }

    let mut out = String::from("<imports>\n");
    for edge in outbound.iter().take(MAX_NEIGHBORS) {
        out.push_str(&format!("imports {}\n", describe_node(graph, edge.to)));
    }
    for edge in inbound.iter().take(MAX_NEIGHBORS) {
        out.push_str(&format!(
            "imported_by {}\n",
            describe_node(graph, edge.from)
        ));
    }
    out.push_str("</imports>\n");
    Some(out)
}

fn call_block(graph: &dyn GraphReader, node: NodeId) -> Option<String> {
    let outbound = graph
        .outbound(node, Some(EdgeKind::Calls))
        .unwrap_or_default();
    let inbound = graph
        .inbound(node, Some(EdgeKind::Calls))
        .unwrap_or_default();
    if outbound.is_empty() && inbound.is_empty() {
        return None;
    }

    let mut out = String::from("<calls>\n");
    for edge in outbound.iter().take(MAX_NEIGHBORS) {
        out.push_str(&format!("calls {}\n", describe_node(graph, edge.to)));
    }
    for edge in inbound.iter().take(MAX_NEIGHBORS) {
        out.push_str(&format!("called_by {}\n", describe_node(graph, edge.from)));
    }
    out.push_str("</calls>\n");
    Some(out)
}

fn exported_symbols_block(
    graph: &dyn GraphReader,
    target: &CommentaryContextTarget,
) -> Option<String> {
    let symbols = graph.symbols_for_file(target.file.id).ok()?;
    let visible = symbols
        .into_iter()
        .filter(|symbol| {
            matches!(symbol.visibility, Visibility::Public | Visibility::Crate)
                || symbol.kind == SymbolKind::Export
        })
        .take(MAX_EXPORTED_SYMBOLS)
        .collect::<Vec<_>>();
    if visible.is_empty() {
        return None;
    }

    let mut out = String::from("<exported_symbols>\n");
    for symbol in visible {
        let signature = symbol.signature.unwrap_or_default();
        out.push_str(&format!(
            "{} {} visibility={} at {}:{} {}\n",
            symbol.kind.as_str(),
            symbol.qualified_name,
            symbol.visibility.as_str(),
            target.file.path,
            symbol.body_byte_range.0,
            signature
        ));
    }
    out.push_str("</exported_symbols>\n");
    Some(out)
}

fn associated_tests_block(graph: &dyn GraphReader, source_path: &str) -> Option<String> {
    let paths = graph.all_file_paths().ok()?;
    let mut matches = paths
        .into_iter()
        .map(|(path, _id)| path)
        .filter(|path| looks_like_associated_test(path, source_path))
        .take(MAX_ASSOCIATED_TESTS)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    matches.sort();
    let mut out = String::from("<associated_tests>\n");
    for path in matches {
        out.push_str(&format!("{path}\n"));
    }
    out.push_str("</associated_tests>\n");
    Some(out)
}

fn governing_decisions_block(graph: &dyn GraphReader, node: NodeId) -> Option<String> {
    let concepts = graph.find_governing_concepts(node).ok()?;
    if concepts.is_empty() {
        return None;
    }
    let mut out = String::from("<governing_decisions>\n");
    for concept in concepts.into_iter().take(MAX_NEIGHBORS) {
        out.push_str(&format!("{} at {}", concept.title, concept.path));
        if let Some(status) = concept.status {
            out.push_str(&format!(" status={status}"));
        }
        if let Some(body) = concept.decision_body {
            out.push_str(&format!(" decision={}", truncate_chars(&body, 500)));
        }
        out.push('\n');
    }
    out.push_str("</governing_decisions>\n");
    Some(out)
}

fn co_change_block(graph: &dyn GraphReader, target: &CommentaryContextTarget) -> Option<String> {
    let file_node = NodeId::File(target.file.id);
    let outbound = graph
        .outbound(file_node, Some(EdgeKind::CoChangesWith))
        .unwrap_or_default();
    let inbound = graph
        .inbound(file_node, Some(EdgeKind::CoChangesWith))
        .unwrap_or_default();
    if outbound.is_empty() && inbound.is_empty() {
        return None;
    }
    let mut out = String::from("<co_change_partners>\n");
    for edge in outbound.iter().chain(inbound.iter()).take(MAX_NEIGHBORS) {
        let other = if edge.from == file_node {
            edge.to
        } else {
            edge.from
        };
        out.push_str(&format!("{}\n", describe_node(graph, other)));
    }
    out.push_str("</co_change_partners>\n");
    Some(out)
}

fn open_markers_block(source: &str) -> Option<String> {
    let mut out = String::new();
    let mut emitted = 0usize;
    for (idx, line) in source.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        if !(lower.contains("todo")
            || lower.contains("fixme")
            || lower.contains("unimplemented")
            || lower.contains("dead code"))
        {
            continue;
        }
        if emitted == 0 {
            out.push_str("<open_markers>\n");
        }
        out.push_str(&format!("line {}: {}\n", idx + 1, line.trim()));
        emitted += 1;
        if emitted >= MAX_MARKERS {
            break;
        }
    }
    if emitted == 0 {
        return None;
    }
    out.push_str("</open_markers>\n");
    Some(out)
}

fn dependency_sources_block(
    repo_root: &Path,
    graph: &dyn GraphReader,
    target: &CommentaryContextTarget,
) -> Option<String> {
    let imports = graph
        .outbound(NodeId::File(target.file.id), Some(EdgeKind::Imports))
        .ok()?;
    let mut out = String::new();
    let mut added = 0usize;
    for edge in imports {
        if added >= MAX_RELATED_FILES {
            break;
        }
        let NodeId::File(file_id) = edge.to else {
            continue;
        };
        let Ok(Some(file)) = graph.get_file(file_id) else {
            continue;
        };
        let Some(source) = read_limited(repo_root, &file.path, MAX_RELATED_CHARS) else {
            continue;
        };
        out.push_str(&format!(
            "<dependency_source path=\"{}\">\n{}\n</dependency_source>\n",
            file.path, source
        ));
        added += 1;
    }
    if added == 0 {
        None
    } else {
        Some(out)
    }
}

fn module_tree_block(repo_root: &Path, source_path: &str) -> Option<String> {
    let parent = Path::new(source_path).parent()?;
    let root = repo_root.join(parent);
    let entries = fs::read_dir(&root).ok()?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return None;
    }

    let mut out = format!("<module_tree root=\"{}\">\n", parent.to_string_lossy());
    for path in paths.into_iter().take(MAX_TREE_ENTRIES) {
        if let Ok(relative) = path.strip_prefix(repo_root) {
            out.push_str(&format!("{}\n", relative.display()));
        }
    }
    out.push_str("</module_tree>\n");
    Some(out)
}

fn module_peer_sources_block(repo_root: &Path, source_path: &str) -> Option<String> {
    let parent = Path::new(source_path).parent()?;
    let root = repo_root.join(parent);
    let entries = fs::read_dir(&root).ok()?;
    let target = repo_root.join(source_path);
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter(|path| path != &target)
        .collect();
    paths.sort();

    let mut out = String::new();
    let mut added = 0usize;
    for path in paths.into_iter().take(MAX_MODULE_FILES) {
        let Ok(relative) = path.strip_prefix(repo_root) else {
            continue;
        };
        let relative = relative.to_string_lossy();
        let Some(source) = read_limited(repo_root, &relative, MAX_RELATED_CHARS) else {
            continue;
        };
        out.push_str(&format!(
            "<module_peer_source path=\"{}\">\n{}\n</module_peer_source>\n",
            relative, source
        ));
        added += 1;
    }
    if added == 0 {
        None
    } else {
        Some(out)
    }
}

fn describe_node(graph: &dyn GraphReader, node: NodeId) -> String {
    match node {
        NodeId::File(file_id) => graph
            .get_file(file_id)
            .ok()
            .flatten()
            .map(|file| format!("file {} ({})", file.path, file_id))
            .unwrap_or_else(|| format!("file {file_id}")),
        NodeId::Symbol(symbol_id) => graph
            .get_symbol(symbol_id)
            .ok()
            .flatten()
            .map(|symbol| {
                let path = graph
                    .get_file(symbol.file_id)
                    .ok()
                    .flatten()
                    .map(|file| file.path)
                    .unwrap_or_else(|| "unknown".to_string());
                format!(
                    "symbol {} kind={} visibility={} at {}:{} ({})",
                    symbol.qualified_name,
                    symbol.kind.as_str(),
                    symbol.visibility.as_str(),
                    path,
                    symbol.body_byte_range.0,
                    symbol_id
                )
            })
            .unwrap_or_else(|| format!("symbol {symbol_id}")),
        NodeId::Concept(concept_id) => graph
            .get_concept(concept_id)
            .ok()
            .flatten()
            .map(|concept| format!("concept {} at {}", concept.title, concept.path))
            .unwrap_or_else(|| format!("concept {concept_id}")),
    }
}

fn read_limited(repo_root: &Path, repo_relative: &str, limit: usize) -> Option<String> {
    let text = fs::read_to_string(repo_root.join(repo_relative)).ok()?;
    Some(truncate_chars(&text, limit))
}

fn is_module_root(path: &str) -> bool {
    path.ends_with("/mod.rs") || path.ends_with("/lib.rs") || path == "lib.rs"
}

fn looks_like_associated_test(path: &str, source_path: &str) -> bool {
    let Some((source_dir, source_name)) = source_path.rsplit_once('/') else {
        return false;
    };
    let source_stem = source_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(source_name);
    let test_name = path.rsplit_once('/').map(|(_, name)| name).unwrap_or(path);
    let same_dir_test = path.starts_with(&format!("{source_dir}/tests/"))
        || path.starts_with(&format!("{source_dir}/__tests__/"));
    same_dir_test
        || test_name.starts_with(&format!("{source_stem}_test"))
        || test_name.starts_with(&format!("test_{source_stem}"))
        || test_name.starts_with(&format!("{source_stem}.test"))
        || test_name.starts_with(&format!("{source_stem}.spec"))
        || path == format!("tests/{source_stem}.rs")
        || path == format!("tests/{source_stem}.py")
}
