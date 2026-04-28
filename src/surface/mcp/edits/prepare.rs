use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{core::ids::NodeId, structure::graph::snapshot, surface::card::CardCompiler};

use super::{anchor_manager, AnchorLine, PreparedAnchorState};
use crate::surface::mcp::{helpers::render_result, SynrepoState};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PrepareEditContextParams {
    pub repo_root: Option<PathBuf>,
    /// File path, symbol name, node ID, or path used with start_line/end_line.
    pub target: String,
    /// Optional target kind: file/path, symbol, or range.
    #[serde(default)]
    pub target_kind: Option<String>,
    /// 1-based start line for range targets.
    #[serde(default)]
    pub start_line: Option<usize>,
    /// 1-based inclusive end line for range targets.
    #[serde(default)]
    pub end_line: Option<usize>,
    /// Caller task identifier. Generated when absent.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Maximum prepared lines to return. Defaults to 80.
    #[serde(default)]
    pub budget_lines: Option<usize>,
}

pub fn handle_prepare_edit_context(
    state: &SynrepoState,
    params: PrepareEditContextParams,
) -> String {
    render_result(prepare_edit_context(state, params))
}

fn prepare_edit_context(
    state: &SynrepoState,
    params: PrepareEditContextParams,
) -> anyhow::Result<serde_json::Value> {
    let compiler = state
        .create_read_compiler()
        .map_err(|e| anyhow::anyhow!(e))?;
    let target = resolve_prepare_target(state, &compiler, &params)?;
    let abs_path = state.repo_root.join(&target.path);
    let content = fs::read_to_string(&abs_path)
        .map_err(|err| anyhow::anyhow!("failed to read {}: {err}", target.path))?;
    let current_content_hash = hash_bytes(content.as_bytes());
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        anyhow::bail!("target file is empty: {}", target.path);
    }

    let max_lines = params.budget_lines.unwrap_or(80).max(1);
    let start = target.start_line.max(1).min(lines.len());
    let end = target.end_line.max(start).min(lines.len());
    let capped_end = end.min(start.saturating_add(max_lines).saturating_sub(1));
    let mut anchors = Vec::new();
    for line_number in start..=capped_end {
        anchors.push(AnchorLine {
            anchor: format!("L{line_number:06}"),
            line: line_number,
            text: lines[line_number - 1].to_string(),
        });
    }

    let task_id = params.task_id.unwrap_or_else(|| {
        let digest = blake3::hash(
            format!("{}:{current_content_hash}:{start}:{end}", target.path).as_bytes(),
        )
        .to_hex()
        .to_string();
        format!("task-{}", &digest[..12])
    });
    let anchor_state_version =
        anchor_manager().next_version(&state.repo_root, &target.path, &current_content_hash);
    let source_text = anchors
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let source_hash = hash_bytes(source_text.as_bytes());

    anchor_manager().insert(PreparedAnchorState {
        repo_root: state.repo_root.clone(),
        task_id: task_id.clone(),
        path: target.path.clone(),
        content_hash: current_content_hash.clone(),
        anchor_state_version: anchor_state_version.clone(),
        anchors: anchors.clone(),
    });

    Ok(json!({
        "task_id": task_id,
        "anchor_state_version": anchor_state_version,
        "repo_root": state.repo_root,
        "graph_epoch": graph_epoch(),
        "path": target.path,
        "file_id": target.file_id,
        "symbol_id": target.symbol_id,
        "content_hash": current_content_hash,
        "graph_content_hash": target.graph_content_hash,
        "source_hash": source_hash,
        "range": {
            "start_line": start,
            "end_line": capped_end,
            "requested_end_line": end,
            "truncated": capped_end < end,
        },
        "anchors": anchors,
        "context": source_text,
        "anchor_state": "session_scoped_operational_cache",
    }))
}

struct PrepareTarget {
    path: String,
    file_id: Option<String>,
    symbol_id: Option<String>,
    graph_content_hash: Option<String>,
    start_line: usize,
    end_line: usize,
}

fn resolve_prepare_target(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    params: &PrepareEditContextParams,
) -> anyhow::Result<PrepareTarget> {
    let kind = params
        .target_kind
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    if kind == "range" || params.start_line.is_some() || params.end_line.is_some() {
        let path = normalize_rel_path(&state.repo_root, &params.target)?;
        let file = compiler.reader().file_by_path(&path)?;
        return Ok(PrepareTarget {
            path,
            file_id: file.as_ref().map(|f| f.id.to_string()),
            symbol_id: None,
            graph_content_hash: file.map(|f| f.content_hash),
            start_line: params.start_line.unwrap_or(1),
            end_line: params.end_line.unwrap_or(params.start_line.unwrap_or(1)),
        });
    }

    if kind == "file" || kind == "path" {
        let path = normalize_rel_path(&state.repo_root, &params.target)?;
        let file = compiler.reader().file_by_path(&path)?;
        return Ok(PrepareTarget {
            path,
            file_id: file.as_ref().map(|f| f.id.to_string()),
            symbol_id: None,
            graph_content_hash: file.map(|f| f.content_hash),
            start_line: 1,
            end_line: usize::MAX,
        });
    }

    let node = compiler
        .resolve_target(&params.target)?
        .ok_or_else(|| anyhow::anyhow!("target not found: {}", params.target))?;
    match node {
        NodeId::File(file_id) => {
            let file = compiler
                .reader()
                .get_file(file_id)?
                .ok_or_else(|| anyhow::anyhow!("file not found for id {file_id}"))?;
            Ok(PrepareTarget {
                path: file.path,
                file_id: Some(file.id.to_string()),
                symbol_id: None,
                graph_content_hash: Some(file.content_hash),
                start_line: 1,
                end_line: usize::MAX,
            })
        }
        NodeId::Symbol(symbol_id) => {
            let symbol = compiler
                .reader()
                .get_symbol(symbol_id)?
                .ok_or_else(|| anyhow::anyhow!("symbol not found for id {symbol_id}"))?;
            let file = compiler
                .reader()
                .get_file(symbol.file_id)?
                .ok_or_else(|| anyhow::anyhow!("file not found for symbol {symbol_id}"))?;
            let content = fs::read_to_string(state.repo_root.join(&file.path))?;
            let (start_line, end_line) = byte_range_to_lines(&content, symbol.body_byte_range);
            Ok(PrepareTarget {
                path: file.path,
                file_id: Some(file.id.to_string()),
                symbol_id: Some(symbol.id.to_string()),
                graph_content_hash: Some(file.content_hash),
                start_line,
                end_line,
            })
        }
        NodeId::Concept(_) => anyhow::bail!("concept targets cannot be edited as source"),
    }
}

pub(crate) fn normalize_rel_path(repo_root: &Path, input: &str) -> anyhow::Result<String> {
    let raw = Path::new(input);
    let rel = if raw.is_absolute() {
        raw.strip_prefix(repo_root)
            .map_err(|_| anyhow::anyhow!("path is outside repo root: {input}"))?
    } else {
        raw
    };
    if rel.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        anyhow::bail!("path must stay within the repo root: {input}");
    }
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn byte_range_to_lines(content: &str, range: (u32, u32)) -> (usize, usize) {
    let start_byte = range.0 as usize;
    let end_byte = range.1 as usize;
    let start_line = content[..start_byte.min(content.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    let end_line = content[..end_byte.min(content.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    (start_line, end_line.max(start_line))
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn graph_epoch() -> Option<u64> {
    let epoch = snapshot::current().snapshot_epoch;
    (epoch > 0).then_some(epoch)
}
