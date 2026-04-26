//! Prompt context assembly for commentary refresh.

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::ids::NodeId;
use crate::pipeline::repair::commentary::CommentaryNodeSnapshot;
use crate::structure::graph::{EdgeKind, GraphReader};

const MAX_SOURCE_CHARS: usize = 16_000;
const MAX_RELATED_FILES: usize = 5;
const MAX_RELATED_CHARS: usize = 2_000;
const MAX_MODULE_FILES: usize = 8;
const MAX_TREE_ENTRIES: usize = 80;

pub(super) fn build_context_text(
    repo_root: &Path,
    graph: &dyn GraphReader,
    snap: &CommentaryNodeSnapshot,
) -> String {
    let mut out = String::new();
    match &snap.symbol {
        Some(sym) => {
            out.push_str(&format!(
                "Target kind: symbol\nSymbol: {}\nSource path: {}\n",
                sym.qualified_name, snap.file.path
            ));
            if let Some(signature) = &sym.signature {
                out.push_str(&format!("Signature: {signature}\n"));
            }
            if let Some(doc) = &sym.doc_comment {
                out.push_str(&format!("<doc_comment>\n{doc}\n</doc_comment>\n"));
            }
        }
        None => {
            out.push_str(&format!(
                "Target kind: file\nSource path: {}\nOnly explain this file. Use related files as context, not as the target.\n",
                snap.file.path
            ));
        }
    }

    if let Some(source) = read_limited(repo_root, &snap.file.path, MAX_SOURCE_CHARS) {
        out.push_str(&format!(
            "<source_code path=\"{}\">\n{}\n</source_code>\n",
            snap.file.path, source
        ));
    }

    append_related_files(&mut out, repo_root, graph, snap);
    if is_module_root(&snap.file.path) {
        append_module_context(&mut out, repo_root, &snap.file.path);
        append_module_sources(&mut out, repo_root, &snap.file.path);
    }
    out
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
