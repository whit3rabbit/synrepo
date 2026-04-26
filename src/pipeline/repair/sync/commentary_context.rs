//! Prompt context assembly for commentary refresh.

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::ids::NodeId;
use crate::pipeline::explain::commentary_template::build_commentary_context;
use crate::pipeline::repair::commentary::CommentaryNodeSnapshot;
use crate::structure::graph::{EdgeKind, GraphReader};

const MAX_SOURCE_CHARS: usize = 16_000;
const MAX_RELATED_FILES: usize = 5;
const MAX_RELATED_CHARS: usize = 2_000;
const MAX_MODULE_FILES: usize = 8;
const MAX_TREE_ENTRIES: usize = 80;
const MAX_ASSOCIATED_NODES: usize = 10;
const MAX_ASSOCIATED_TESTS: usize = 10;
const MAX_MARKERS: usize = 12;

pub(super) fn build_context_text(
    repo_root: &Path,
    graph: &dyn GraphReader,
    snap: &CommentaryNodeSnapshot,
) -> String {
    let target_node = target_node_id(snap);
    let mut target = format!("Target node: {target_node}\n");
    let mut out = String::new();
    match &snap.symbol {
        Some(sym) => {
            target.push_str(&format!(
                "Target kind: symbol\nSymbol: {}\nSource path: {}\n",
                sym.qualified_name, snap.file.path
            ));
            if let Some(signature) = &sym.signature {
                target.push_str(&format!("Signature: {signature}\n"));
            }
            if let Some(doc) = &sym.doc_comment {
                out.push_str(&format!("<doc_comment>\n{doc}\n</doc_comment>\n"));
            }
        }
        None => {
            target.push_str(&format!(
                "Target kind: file\nSource path: {}\nOnly explain this file. Use related files as context, not as the target.\n",
                snap.file.path
            ));
        }
    }

    if let Some(source) = read_limited(repo_root, &snap.file.path, MAX_SOURCE_CHARS) {
        append_source_markers(&mut out, &source);
        out.push_str(&format!(
            "<source_code path=\"{}\">\n{}\n</source_code>\n",
            snap.file.path, source
        ));
    }

    append_associated_nodes(&mut out, graph, target_node);
    append_associated_tests(&mut out, graph, &snap.file.path);
    append_related_files(&mut out, repo_root, graph, snap);
    if is_module_root(&snap.file.path) {
        append_module_context(&mut out, repo_root, &snap.file.path);
        append_module_sources(&mut out, repo_root, &snap.file.path);
    }
    build_commentary_context(&target, &out)
}

fn target_node_id(snap: &CommentaryNodeSnapshot) -> NodeId {
    snap.symbol
        .as_ref()
        .map(|sym| NodeId::Symbol(sym.id))
        .unwrap_or(NodeId::File(snap.file.id))
}

fn append_associated_nodes(out: &mut String, graph: &dyn GraphReader, target: NodeId) {
    out.push_str("<associated_nodes>\n");
    let mut emitted = 0usize;
    if let Ok(edges) = graph.outbound(target, None) {
        for edge in edges.into_iter().take(MAX_ASSOCIATED_NODES) {
            out.push_str(&format!(
                "outbound {} -> {} ({})\n",
                edge.from,
                edge.to,
                edge.kind.as_str()
            ));
            emitted += 1;
        }
    }
    if emitted < MAX_ASSOCIATED_NODES {
        if let Ok(edges) = graph.inbound(target, None) {
            for edge in edges.into_iter().take(MAX_ASSOCIATED_NODES - emitted) {
                out.push_str(&format!(
                    "inbound {} -> {} ({})\n",
                    edge.from,
                    edge.to,
                    edge.kind.as_str()
                ));
                emitted += 1;
            }
        }
    }
    if emitted == 0 {
        out.push_str("none found\n");
    }
    out.push_str("</associated_nodes>\n");
}

fn append_associated_tests(out: &mut String, graph: &dyn GraphReader, source_path: &str) {
    out.push_str("<associated_tests>\n");
    let Ok(paths) = graph.all_file_paths() else {
        out.push_str("unavailable\n</associated_tests>\n");
        return;
    };
    let mut emitted = 0usize;
    for (path, _id) in paths {
        if !looks_like_associated_test(&path, source_path) {
            continue;
        }
        out.push_str(&format!("{path}\n"));
        emitted += 1;
        if emitted >= MAX_ASSOCIATED_TESTS {
            break;
        }
    }
    if emitted == 0 {
        out.push_str("none found by path convention\n");
    }
    out.push_str("</associated_tests>\n");
}

fn append_source_markers(out: &mut String, source: &str) {
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
    if emitted > 0 {
        out.push_str("</open_markers>\n");
    }
}

fn append_related_files(
    out: &mut String,
    repo_root: &Path,
    graph: &dyn GraphReader,
    snap: &CommentaryNodeSnapshot,
) {
    let Ok(imports) = graph.outbound(NodeId::File(snap.file.id), Some(EdgeKind::Imports)) else {
        return;
    };
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
        if let Some(source) = read_limited(repo_root, &file.path, MAX_RELATED_CHARS) {
            out.push_str(&format!(
                "<dependency_source path=\"{}\">\n{}\n</dependency_source>\n",
                file.path, source
            ));
            added += 1;
        }
    }
}

fn append_module_context(out: &mut String, repo_root: &Path, source_path: &str) {
    let Some(parent) = Path::new(source_path).parent() else {
        return;
    };
    let root = repo_root.join(parent);
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };

    out.push_str(&format!(
        "<module_tree root=\"{}\">\n",
        parent.to_string_lossy()
    ));
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    for path in paths.into_iter().take(MAX_TREE_ENTRIES) {
        if let Ok(relative) = path.strip_prefix(repo_root) {
            out.push_str(&format!("{}\n", relative.display()));
        }
    }
    out.push_str("</module_tree>\n");
}

fn append_module_sources(out: &mut String, repo_root: &Path, source_path: &str) {
    let Some(parent) = Path::new(source_path).parent() else {
        return;
    };
    let root = repo_root.join(parent);
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    paths.sort();
    let target = repo_root.join(source_path);
    for path in paths
        .into_iter()
        .filter(|path| path != &target)
        .take(MAX_MODULE_FILES)
    {
        let Ok(relative) = path.strip_prefix(repo_root) else {
            continue;
        };
        let relative = relative.to_string_lossy();
        if let Some(source) = read_limited(repo_root, &relative, MAX_RELATED_CHARS) {
            out.push_str(&format!(
                "<module_peer_source path=\"{}\">\n{}\n</module_peer_source>\n",
                relative, source
            ));
        }
    }
}

fn read_limited(repo_root: &Path, repo_relative: &str, limit: usize) -> Option<String> {
    let text = fs::read_to_string(repo_root.join(repo_relative)).ok()?;
    if text.len() <= limit {
        return Some(text);
    }
    let mut truncated = text.chars().take(limit).collect::<String>();
    truncated.push_str("\n/* truncated */");
    Some(truncated)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::explain::commentary_template::REQUIRED_SECTIONS;

    #[test]
    fn associated_test_heuristic_matches_common_patterns() {
        assert!(looks_like_associated_test("src/foo_test.rs", "src/foo.rs"));
        assert!(looks_like_associated_test("src/tests/foo.rs", "src/foo.rs"));
        assert!(looks_like_associated_test("tests/foo.py", "src/foo.py"));
        assert!(!looks_like_associated_test("src/bar_test.rs", "src/foo.rs"));
    }

    #[test]
    fn template_sections_are_available_to_context_builder() {
        let target = "Target node: file_1\nTarget kind: file\n";
        let context = build_commentary_context(target, "<source_code>body</source_code>");
        for section in REQUIRED_SECTIONS {
            assert!(context.contains(section));
        }
    }
}
