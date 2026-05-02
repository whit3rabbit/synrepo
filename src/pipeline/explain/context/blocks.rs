//! Optional graph context blocks for commentary prompts.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::ids::{FileNodeId, NodeId};
use crate::structure::graph::{EdgeKind, GraphReader, SymbolKind, Visibility};
use crate::util::test_paths;

use super::describe::NodeDescriptions;
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
    let file_node = NodeId::File(target.file.id);
    let imports_out = graph
        .outbound(file_node, Some(EdgeKind::Imports))
        .unwrap_or_default();
    let imports_in = graph
        .inbound(file_node, Some(EdgeKind::Imports))
        .unwrap_or_default();
    let calls_out = graph
        .outbound(target.node_id(), Some(EdgeKind::Calls))
        .unwrap_or_default();
    let calls_in = graph
        .inbound(target.node_id(), Some(EdgeKind::Calls))
        .unwrap_or_default();
    let cochange_out = graph
        .outbound(file_node, Some(EdgeKind::CoChangesWith))
        .unwrap_or_default();
    let cochange_in = graph
        .inbound(file_node, Some(EdgeKind::CoChangesWith))
        .unwrap_or_default();

    // One bulk description fetch covers neighbors across imports, calls, and
    // co-change blocks.
    let descriptions = NodeDescriptions::load(
        graph,
        neighbor_endpoints(file_node, &imports_out, &imports_in)
            .chain(neighbor_endpoints(target.node_id(), &calls_out, &calls_in))
            .chain(neighbor_endpoints(file_node, &cochange_out, &cochange_in)),
    );

    let mut blocks = Vec::new();
    push_if_some(
        &mut blocks,
        edges_block(
            "imports",
            "imported_by",
            &imports_out,
            &imports_in,
            &descriptions,
        ),
    );
    push_if_some(
        &mut blocks,
        edges_block("calls", "called_by", &calls_out, &calls_in, &descriptions),
    );
    push_if_some(&mut blocks, exported_symbols_block(graph, target));
    push_if_some(
        &mut blocks,
        associated_tests_block(graph, &target.file.path),
    );
    push_if_some(
        &mut blocks,
        governing_decisions_block(graph, target.node_id()),
    );
    push_if_some(
        &mut blocks,
        co_change_block(file_node, &cochange_out, &cochange_in, &descriptions),
    );
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

fn neighbor_endpoints<'a>(
    anchor: NodeId,
    outbound: &'a [crate::structure::graph::Edge],
    inbound: &'a [crate::structure::graph::Edge],
) -> impl Iterator<Item = NodeId> + 'a {
    outbound
        .iter()
        .take(MAX_NEIGHBORS)
        .map(move |edge| {
            if edge.from == anchor {
                edge.to
            } else {
                edge.from
            }
        })
        .chain(inbound.iter().take(MAX_NEIGHBORS).map(move |edge| {
            if edge.from == anchor {
                edge.to
            } else {
                edge.from
            }
        }))
}

fn edges_block(
    out_label: &str,
    in_label: &str,
    outbound: &[crate::structure::graph::Edge],
    inbound: &[crate::structure::graph::Edge],
    descriptions: &NodeDescriptions,
) -> Option<String> {
    if outbound.is_empty() && inbound.is_empty() {
        return None;
    }
    let tag = out_label;
    let mut out = format!("<{tag}>\n");
    for edge in outbound.iter().take(MAX_NEIGHBORS) {
        out.push_str(&format!("{out_label} {}\n", descriptions.describe(edge.to)));
    }
    for edge in inbound.iter().take(MAX_NEIGHBORS) {
        out.push_str(&format!(
            "{in_label} {}\n",
            descriptions.describe(edge.from)
        ));
    }
    out.push_str(&format!("</{tag}>\n"));
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
        .filter(|path| test_paths::matches_path_convention(path, source_path))
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

fn co_change_block(
    file_node: NodeId,
    outbound: &[crate::structure::graph::Edge],
    inbound: &[crate::structure::graph::Edge],
    descriptions: &NodeDescriptions,
) -> Option<String> {
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
        out.push_str(&format!("{}\n", descriptions.describe(other)));
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
    let file_ids = imports
        .iter()
        .filter_map(|edge| match edge.to {
            NodeId::File(file_id) => Some(file_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    let files_by_id: HashMap<FileNodeId, _> = graph
        .get_files(&file_ids)
        .ok()?
        .into_iter()
        .map(|file| (file.id, file))
        .collect();
    let mut out = String::new();
    let mut added = 0usize;
    for edge in imports {
        if added >= MAX_RELATED_FILES {
            break;
        }
        let NodeId::File(file_id) = edge.to else {
            continue;
        };
        let Some(file) = files_by_id.get(&file_id) else {
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

fn read_limited(repo_root: &Path, repo_relative: &str, limit: usize) -> Option<String> {
    let text = fs::read_to_string(repo_root.join(repo_relative)).ok()?;
    Some(truncate_chars(&text, limit))
}

fn is_module_root(path: &str) -> bool {
    path.ends_with("/mod.rs") || path.ends_with("/lib.rs") || path == "lib.rs"
}
