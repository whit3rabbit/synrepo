use std::collections::HashSet;
use std::process::Command;
use std::time::Instant;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{
    core::ids::NodeId,
    structure::graph::EdgeKind,
    surface::card::{Budget, CardCompiler},
};

use super::helpers::{render_result, with_graph_snapshot};
use super::SynrepoState;

/// Parameters for the `synrepo_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
        pub repo_root: Option<std::path::PathBuf>,
/// Lexical query string.
    pub query: String,
    /// Maximum number of results to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

pub fn default_limit() -> u32 {
    20
}

/// Parameters for the `synrepo_where_to_edit` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WhereToEditParams {
        pub repo_root: Option<std::path::PathBuf>,
/// Plain-language description of the task (e.g. "add retry logic to HTTP client").
    pub task: String,
    /// Maximum number of file suggestions to return. Defaults to 5.
    #[serde(default = "default_edit_limit")]
    pub limit: u32,
    /// Optional numeric token cap for returned card suggestions.
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

pub fn default_edit_limit() -> u32 {
    5
}

/// Parameters for the `synrepo_change_impact` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangeImpactParams {
        pub repo_root: Option<std::path::PathBuf>,
/// Target file path or symbol name to assess change impact for.
    pub target: String,
}

pub fn handle_search(state: &SynrepoState, query: String, limit: u32) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let matches = crate::substrate::search(&state.config, &state.repo_root, &query)?;

        let items: Vec<serde_json::Value> = matches
            .into_iter()
            .take(limit as usize)
            .map(|m| {
                json!({
                    "path": m.path.to_string_lossy(),
                    "line": m.line_number,
                    "content": String::from_utf8_lossy(&m.line_content).trim_end().to_string(),
                })
            })
            .collect();

        Ok(json!({ "query": query, "results": items }))
    })();
    render_result(result)
}

pub fn handle_overview(state: &SynrepoState) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
        let graph_dir = synrepo_dir.join("graph");
        let store = crate::store::sqlite::SqliteGraphStore::open_existing(&graph_dir)?;
        let stats = with_graph_snapshot(&store, || Ok(store.persisted_stats()?))?;
        Ok(json!({
            "mode": state.config.mode.to_string(),
            "graph": {
                "file_nodes": stats.file_nodes,
                "symbol_nodes": stats.symbol_nodes,
                "concept_nodes": stats.concept_nodes,
                "total_edges": stats.total_edges,
                "edges_by_kind": stats.edge_counts_by_kind,
            }
        }))
    })();
    render_result(result)
}

pub fn handle_where_to_edit(
    state: &SynrepoState,
    task: String,
    limit: u32,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let result: anyhow::Result<serde_json::Value> = (|| {
        let matches = crate::substrate::search(&state.config, &state.repo_root, &task)?;

        let compiler = state
            .create_read_compiler()
            .map_err(|e| anyhow::anyhow!(e))?;
        let mut seen = HashSet::new();
        let mut cards = Vec::new();

        for m in &matches {
            let path = m.path.to_string_lossy().to_string();
            if seen.contains(&path) {
                continue;
            }
            seen.insert(path.clone());

            if let Some(file) = compiler.reader().file_by_path(&path)? {
                let card = compiler.file_card(file.id, Budget::Tiny)?;
                cards.push(serde_json::to_value(&card)?);
            }

            if cards.len() >= limit as usize {
                break;
            }
        }

        let (truncation_applied, accountings) = apply_card_set_cap(&mut cards, budget_tokens);
        let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
        let latency_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        crate::pipeline::context_metrics::record_cards_best_effort(
            &synrepo_dir,
            &accountings,
            latency_ms,
            false,
        );

        Ok(json!({
            "task": task,
            "suggestions": cards,
            "truncation_applied": truncation_applied,
        }))
    })();
    render_result(result)
}

pub fn handle_change_impact(state: &SynrepoState, target: String) -> String {
    let result = (|| {
        let compiler = state
            .create_read_compiler()
            .map_err(|e| anyhow::anyhow!(e))?;
        let node_id = compiler
            .resolve_target(&target)?
            .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

        let imports_in = compiler
            .reader()
            .inbound(node_id, Some(EdgeKind::Imports))?;
        let calls_in = compiler.reader().inbound(node_id, Some(EdgeKind::Calls))?;

        let mut impacted_files: Vec<serde_json::Value> = Vec::new();
        let mut seen_files = HashSet::new();

        for edge in imports_in.iter().chain(calls_in.iter()) {
            let file_id = match edge.from {
                NodeId::File(id) => id,
                NodeId::Symbol(sym_id) => {
                    if let Some(sym) = compiler.reader().get_symbol(sym_id)? {
                        sym.file_id
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if seen_files.insert(file_id) {
                if let Some(file) = compiler.reader().get_file(file_id)? {
                    impacted_files.push(json!({
                        "path": file.path,
                        "edge_kind": edge.kind.as_str(),
                    }));
                }
            }
        }

        Ok(json!({
            "target": target,
            "impacted_files": impacted_files,
            "total": impacted_files.len(),
        }))
    })();
    render_result(result)
}

pub fn handle_changed(state: &SynrepoState) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let changed_files = git_changed_files(&state.repo_root)?;
        let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
        crate::pipeline::context_metrics::record_changed_files_best_effort(
            &synrepo_dir,
            changed_files.len(),
        );

        Ok(json!({
            "changed_files": changed_files,
            "changed_file_count": changed_files.len(),
            "index_state": if changed_files.is_empty() { "current_or_unknown" } else { "possibly_stale" },
            "recommended_commands": [
                "synrepo status",
                "synrepo check",
                "synrepo tests <changed-path>",
                "synrepo sync"
            ],
        }))
    })();
    render_result(result)
}

/// Trim a ranked card set to fit under `budget_tokens`, preserving rank order.
/// Always keeps the first (top-ranked) card so callers never get an empty
/// suggestion list, even if that card alone exceeds the cap; in that case it
/// is marked `truncation_applied`. Returns `(truncated, accountings)` where
/// `accountings` is the typed metadata of the retained cards (so callers that
/// record metrics don't need to re-deserialize).
fn apply_card_set_cap(
    cards: &mut Vec<serde_json::Value>,
    budget_tokens: Option<usize>,
) -> (bool, Vec<crate::surface::card::ContextAccounting>) {
    let original_len = cards.len();
    let mut accountings = Vec::with_capacity(cards.len());
    let mut total = 0usize;
    let mut keep = 0usize;
    let mut any_marked = false;

    for (idx, card) in cards.iter_mut().enumerate() {
        let Some(accounting) = card.get("context_accounting").and_then(|v| {
            serde_json::from_value::<crate::surface::card::ContextAccounting>(v.clone()).ok()
        }) else {
            continue;
        };
        let tokens = accounting.token_estimate;

        let over_cap = budget_tokens.is_some_and(|cap| total + tokens > cap);
        if over_cap && idx > 0 {
            break;
        }
        if over_cap {
            mark_truncated(card);
            any_marked = true;
        }
        total += tokens;
        accountings.push(accounting);
        keep = idx + 1;
    }

    cards.truncate(keep);
    (any_marked || cards.len() != original_len, accountings)
}

fn mark_truncated(card: &mut serde_json::Value) {
    if let Some(accounting) = card
        .get_mut("context_accounting")
        .and_then(|v| v.as_object_mut())
    {
        accounting.insert(
            "truncation_applied".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}

fn git_changed_files(repo_root: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    if !output.status.success() {
        return Ok(vec![]);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let path = line[3..].trim();
        let path = path.rsplit(" -> ").next().unwrap_or(path);
        if !path.is_empty() {
            files.push(path.to_string());
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}
