use std::time::Instant;

use serde_json::json;

use super::{
    card_render::render_card_target,
    cards::CardParams,
    helpers::{parse_budget, render_result},
    limits::{MAX_CARD_TARGETS, MAX_DEEP_CARD_TARGETS},
    response_budget::estimate_json_tokens,
    SynrepoState,
};

pub fn handle_degraded_card(
    repo_root: Option<std::path::PathBuf>,
    params: CardParams,
    error: anyhow::Error,
) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let target = params
            .target
            .or_else(|| params.targets.into_iter().next())
            .ok_or_else(|| super::error::McpError::invalid_parameter("target is required"))?;
        let Some(repo_root) = repo_root else {
            return Err(error);
        };
        let abs = repo_root.join(&target);
        let git_status = std::process::Command::new("git")
            .args(["status", "--porcelain", "--", &target])
            .current_dir(&repo_root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
        let path = target.clone();
        Ok(json!({
            "source_store": "degraded",
            "target": target,
            "path": path,
            "repo_root": repo_root,
            "exists": abs.exists(),
            "is_file": abs.is_file(),
            "git_status": git_status,
            "error": super::error::error_value(&error)["error"].clone(),
            "context_accounting": {
                "budget_tier": params.budget,
                "token_estimate": 0,
                "raw_file_token_estimate": 0,
                "estimated_savings_ratio": 0.0,
                "source_hashes": [],
                "stale": true,
                "truncation_applied": false
            }
        }))
    })();
    render_result(result)
}

pub fn handle_card_params(state: &SynrepoState, params: CardParams) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let mut targets = params.targets;
        if let Some(target) = params.target {
            if targets.is_empty() {
                targets.push(target);
            } else {
                targets.insert(0, target);
            }
        }
        if targets.is_empty() {
            return Err(
                super::error::McpError::invalid_parameter("target or targets is required").into(),
            );
        }
        if targets.len() == 1 {
            let budget = parse_budget(&params.budget)?;
            return state
                .with_read_compiler(|compiler| {
                    render_card_target(
                        state,
                        compiler,
                        &targets[0],
                        budget,
                        params.budget_tokens,
                        params.include_notes,
                        Instant::now(),
                    )
                    .map_err(crate::Error::from)
                })
                .map_err(|err| anyhow::anyhow!(err));
        }
        if targets.len() > MAX_CARD_TARGETS {
            return Err(super::error::McpError::invalid_parameter(format!(
                "targets has {} entries, exceeding limit {MAX_CARD_TARGETS}",
                targets.len()
            ))
            .into());
        }
        let budget = parse_budget(&params.budget)?;
        if budget == crate::surface::card::Budget::Deep && targets.len() > MAX_DEEP_CARD_TARGETS {
            return Err(super::error::McpError::invalid_parameter(format!(
                "deep card batches are capped at {MAX_DEEP_CARD_TARGETS} targets; use tiny/normal or narrow the request"
            ))
            .into());
        }
        let mut rendered = Vec::new();
        let mut errors = Vec::new();
        state
            .with_read_compiler(|compiler| {
                for target in targets {
                    match render_card_target(
                        state,
                        compiler,
                        &target,
                        budget,
                        params.budget_tokens,
                        params.include_notes,
                        Instant::now(),
                    ) {
                        Ok(card) => rendered.push((target, card)),
                        Err(error) => errors.push(json!({
                            "target": target,
                            "error": super::error::error_value(&error),
                        })),
                    }
                }
                Ok(())
            })
            .map_err(|err| anyhow::anyhow!(err))?;
        let original_count = rendered.len();
        let (cards, omitted, batch_truncated) = apply_batch_cap(rendered, params.budget_tokens);
        let card_count = cards.len();
        let batch_token_estimate = estimate_json_tokens(&json!({ "cards": &cards }));
        Ok(json!({
            "budget": params.budget,
            "cards": cards,
            "errors": errors,
            "total": card_count,
            "omitted": omitted,
            "context_accounting": {
                "budget_tier": params.budget,
                "token_estimate": batch_token_estimate,
                "raw_file_token_estimate": 0,
                "estimated_savings_ratio": 0.0,
                "source_hashes": [],
                "stale": false,
                "truncation_applied": batch_truncated,
                "original_card_count": original_count
            }
        }))
    })();
    render_result(result)
}

fn apply_batch_cap(
    rendered: Vec<(String, serde_json::Value)>,
    budget_tokens: Option<usize>,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>, bool) {
    let Some(cap) = budget_tokens else {
        return (
            rendered.into_iter().map(|(_, card)| card).collect(),
            Vec::new(),
            false,
        );
    };
    let mut used = 0usize;
    let mut cards = Vec::new();
    let mut omitted = Vec::new();
    let mut truncated = false;
    for (idx, (target, mut card)) in rendered.into_iter().enumerate() {
        let tokens = estimate_json_tokens(&card);
        if used + tokens > cap && idx > 0 {
            omitted.push(json!({
                "target": target,
                "reason": "budget_tokens_exceeded",
            }));
            truncated = true;
            continue;
        }
        if used + tokens > cap {
            mark_card_truncated(&mut card);
            truncated = true;
        }
        used += tokens;
        cards.push(card);
    }
    (cards, omitted, truncated)
}

fn mark_card_truncated(card: &mut serde_json::Value) {
    if let Some(obj) = card
        .get_mut("context_accounting")
        .and_then(|value| value.as_object_mut())
    {
        obj.insert(
            "truncation_applied".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}
