use serde_json::{json, Value};

use crate::{
    core::ids::{FileNodeId, SymbolNodeId},
    structure::graph::GraphReader,
    surface::mcp::SynrepoState,
};

const PRIMARY_ROOT_ID: &str = "primary";

#[derive(Clone)]
struct FileMeta {
    path: String,
    root_id: String,
    is_primary_root: bool,
    file_id: FileNodeId,
}

pub(super) fn lexical_items(
    state: &SynrepoState,
    matches: Vec<crate::substrate::RootedSearchMatch>,
) -> Vec<Value> {
    let rows = matches
        .into_iter()
        .map(|m| {
            json!({
                "path": m.path.to_string_lossy(),
                "root_id": m.root_id,
                "is_primary_root": m.is_primary_root,
                "file_id": Value::Null,
                "line": m.line_number,
                "content": String::from_utf8_lossy(&m.line_content).trim_end().to_string(),
                "source": "lexical",
                "fusion_score": Value::Null,
                "semantic_score": Value::Null,
            })
        })
        .collect();
    enrich_path_rows(state, rows)
}

pub(super) fn hybrid_items(
    state: &SynrepoState,
    rows: Vec<crate::substrate::HybridSearchRow>,
) -> Vec<Value> {
    let fallback_rows = rows.clone();
    state
        .with_read_compiler(|compiler| Ok(hybrid_items_with_graph(rows, Some(compiler.reader()))))
        .unwrap_or_else(|_| hybrid_items_with_graph(fallback_rows, None))
}

fn hybrid_items_with_graph(
    rows: Vec<crate::substrate::HybridSearchRow>,
    graph: Option<&dyn GraphReader>,
) -> Vec<Value> {
    rows.into_iter()
        .map(|row| {
            let meta = row
                .file_id
                .and_then(|id| file_meta_by_id(graph, id))
                .or_else(|| row.symbol_id.and_then(|id| symbol_file_meta(graph, id)))
                .or_else(|| {
                    row.path
                        .as_deref()
                        .and_then(|path| file_meta_by_path(graph, row.root_id.as_deref(), path))
                });
            let path = row
                .path
                .clone()
                .or_else(|| meta.as_ref().map(|m| m.path.clone()));
            let root_id = row
                .root_id
                .clone()
                .or_else(|| meta.as_ref().map(|m| m.root_id.clone()));
            let is_primary_root = row
                .is_primary_root
                .or_else(|| meta.as_ref().map(|m| m.is_primary_root));
            let file_id = row.file_id.or_else(|| meta.as_ref().map(|m| m.file_id));
            json!({
                "path": path,
                "root_id": root_id,
                "is_primary_root": is_primary_root,
                "file_id": file_id,
                "line": row.line,
                "content": row.content,
                "source": row.source.as_str(),
                "fusion_score": row.fusion_score,
                "semantic_score": row.semantic_score,
                "chunk_id": row.chunk_id,
                "symbol_id": row.symbol_id,
            })
        })
        .collect()
}

fn enrich_path_rows(state: &SynrepoState, rows: Vec<Value>) -> Vec<Value> {
    let fallback = rows.clone();
    state
        .with_read_compiler(|compiler| {
            Ok(rows
                .into_iter()
                .map(|row| enrich_one_path_row(compiler.reader(), row))
                .collect())
        })
        .unwrap_or(fallback)
}

fn enrich_one_path_row(graph: &dyn GraphReader, mut row: Value) -> Value {
    let path = row.get("path").and_then(Value::as_str).map(str::to_string);
    let root_id = row
        .get("root_id")
        .and_then(Value::as_str)
        .unwrap_or(PRIMARY_ROOT_ID)
        .to_string();
    let Some(path) = path else {
        return row;
    };
    let Some(meta) = file_meta_by_path(Some(graph), Some(&root_id), &path) else {
        return row;
    };
    if let Some(obj) = row.as_object_mut() {
        obj.insert("file_id".to_string(), json!(meta.file_id));
        obj.insert("root_id".to_string(), json!(meta.root_id));
        obj.insert("is_primary_root".to_string(), json!(meta.is_primary_root));
    }
    row
}

fn symbol_file_meta(graph: Option<&dyn GraphReader>, id: SymbolNodeId) -> Option<FileMeta> {
    let graph = graph?;
    let symbol = graph.get_symbol(id).ok().flatten()?;
    file_meta_by_id(Some(graph), symbol.file_id)
}

fn file_meta_by_id(graph: Option<&dyn GraphReader>, id: FileNodeId) -> Option<FileMeta> {
    let graph = graph?;
    let file = graph.get_file(id).ok().flatten()?;
    Some(FileMeta {
        path: file.path,
        root_id: file.root_id.clone(),
        is_primary_root: file.root_id == PRIMARY_ROOT_ID,
        file_id: file.id,
    })
}

fn file_meta_by_path(
    graph: Option<&dyn GraphReader>,
    root_id: Option<&str>,
    path: &str,
) -> Option<FileMeta> {
    let graph = graph?;
    let file = match root_id {
        Some(root_id) => graph.file_by_root_path(root_id, path).ok().flatten()?,
        None => graph.file_by_path(path).ok().flatten()?,
    };
    Some(FileMeta {
        path: file.path,
        root_id: file.root_id.clone(),
        is_primary_root: file.root_id == PRIMARY_ROOT_ID,
        file_id: file.id,
    })
}
