use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

use crate::{
    core::ids::NodeId,
    surface::card::{
        accounting::estimate_tokens_bytes, concept::concept_summary_card, Budget, CardCompiler,
        ContextAccounting,
    },
};

use super::{
    card_accounting::{prepare_card_json, record_embedded_card_metrics},
    helpers::{attach_decision_cards, lift_commentary_text},
    response_budget::estimate_json_tokens,
    SynrepoState,
};

pub fn render_card_target(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
    budget: Budget,
    budget_tokens: Option<usize>,
    include_notes: bool,
    start: Instant,
) -> anyhow::Result<Value> {
    let mut last = None;
    for &candidate in downgrade_budgets(budget) {
        let json = render_card_content(state, compiler, target, candidate, include_notes)?;
        let json = prepare_card_json(state, json, budget_tokens);
        let fits = fits_budget_tokens(&json, budget_tokens);
        last = Some((json, candidate));
        if fits || candidate == Budget::Tiny {
            break;
        }
    }
    let (json, _) = last.expect("downgrade_budgets returns at least one budget");
    record_embedded_card_metrics(state, &json, start, false);
    Ok(json)
}

fn render_card_content(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
    budget: Budget,
    include_notes: bool,
) -> anyhow::Result<Value> {
    let Some(node_id) = compiler.resolve_target(target)? else {
        return filesystem_fallback_card(state, compiler, target, budget)?.ok_or_else(|| {
            super::error::McpError::not_found(format!("target not found: {target}")).into()
        });
    };

    match node_id {
        NodeId::Symbol(sym_id) => {
            let card = compiler.symbol_card(sym_id, budget)?;
            let mut json_val = serde_json::to_value(&card)?;
            lift_commentary_text(&mut json_val);
            attach_decision_cards(
                &mut json_val,
                NodeId::Symbol(sym_id),
                compiler.reader(),
                budget,
            )?;
            if include_notes {
                super::notes::attach_agent_notes(state, &mut json_val, NodeId::Symbol(sym_id))?;
            }
            Ok(json_val)
        }
        NodeId::File(file_id) => {
            let card = compiler.file_card(file_id, budget)?;
            let mut json_val = serde_json::to_value(&card)?;
            attach_decision_cards(
                &mut json_val,
                NodeId::File(file_id),
                compiler.reader(),
                budget,
            )?;
            if include_notes {
                super::notes::attach_agent_notes(state, &mut json_val, NodeId::File(file_id))?;
            }
            Ok(json_val)
        }
        NodeId::Concept(concept_id) => {
            let concept = compiler
                .reader()
                .get_concept(concept_id)?
                .ok_or_else(|| anyhow::anyhow!("concept not found"))?;
            Ok(concept_summary_card(&concept))
        }
    }
}

fn filesystem_fallback_card(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
    budget: Budget,
) -> anyhow::Result<Option<Value>> {
    let Some((absolute, repo_relative)) = repo_existing_path(&state.repo_root, target) else {
        return Ok(None);
    };
    if absolute.is_dir() {
        let card = compiler.module_card(&repo_relative, budget)?;
        return Ok(Some(serde_json::to_value(card)?));
    }
    if !absolute.is_file() {
        return Ok(None);
    }
    let Ok(content) = fs::read_to_string(&absolute) else {
        return Ok(None);
    };
    let metadata = fs::metadata(&absolute)?;
    let headings = markdown_headings(&content);
    let preview = bounded_preview(&content, budget);
    let preview_truncated = preview.len() < content.len();
    let mut value = json!({
        "card_type": "filesystem_fallback",
        "graph_backed": false,
        "source_store": "filesystem",
        "target": target,
        "path": repo_relative,
        "size_bytes": metadata.len(),
        "headings": headings,
        "preview": preview,
        "next_steps": [
            "Run `synrepo reconcile` if this file should be present in graph-backed cards.",
            "Use `synrepo_search` with `literal: true` for exact text matches inside this file."
        ],
        "context_accounting": ContextAccounting::new(
            budget,
            1,
            estimate_tokens_bytes(metadata.len() as usize),
            Vec::new(),
        ).with_truncation(preview_truncated),
    });
    let token_estimate = super::response_budget::estimate_json_tokens(&value);
    if let Some(accounting) = value
        .get_mut("context_accounting")
        .and_then(Value::as_object_mut)
    {
        accounting.insert("token_estimate".to_string(), json!(token_estimate));
    }
    Ok(Some(value))
}

fn repo_existing_path(repo_root: &Path, target: &str) -> Option<(PathBuf, String)> {
    if target.trim().is_empty() {
        return None;
    }
    let repo_root = repo_root.canonicalize().ok()?;
    let raw = Path::new(target);
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        repo_root.join(raw)
    }
    .canonicalize()
    .ok()?;
    if !absolute.starts_with(&repo_root) {
        return None;
    }
    let repo_relative = absolute
        .strip_prefix(&repo_root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    Some((absolute, repo_relative))
}

fn markdown_headings(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('#') {
                return None;
            }
            let heading = trimmed.trim_start_matches('#').trim();
            if heading.is_empty() {
                None
            } else {
                Some(heading.to_string())
            }
        })
        .take(12)
        .collect()
}

fn bounded_preview(content: &str, budget: Budget) -> String {
    let max_chars = match budget {
        Budget::Tiny => 900,
        Budget::Normal => 1_800,
        Budget::Deep => 3_600,
    };
    let mut preview = String::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        if !preview.is_empty() {
            preview.push('\n');
        }
        preview.push_str(line);
        if preview.chars().count() >= max_chars {
            break;
        }
    }
    match preview.char_indices().nth(max_chars) {
        None => preview,
        Some((end, _)) => format!("{}...", &preview[..end]),
    }
}

fn downgrade_budgets(budget: Budget) -> &'static [Budget] {
    match budget {
        Budget::Deep => &[Budget::Deep, Budget::Normal, Budget::Tiny],
        Budget::Normal => &[Budget::Normal, Budget::Tiny],
        Budget::Tiny => &[Budget::Tiny],
    }
}

fn fits_budget_tokens(json: &Value, budget_tokens: Option<usize>) -> bool {
    budget_tokens.is_none_or(|cap| estimate_json_tokens(json) <= cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_fit_uses_final_serialized_json_size() {
        let json = serde_json::json!({
            "context_accounting": {
                "token_estimate": 1
            },
            "payload": "x".repeat(120)
        });
        let serialized_estimate = estimate_json_tokens(&json);

        assert!(serialized_estimate > 1);
        assert!(!fits_budget_tokens(&json, Some(1)));
        assert!(fits_budget_tokens(&json, Some(serialized_estimate)));
    }
}
