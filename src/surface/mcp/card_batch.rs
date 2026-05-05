use std::time::Instant;

use serde_json::json;

use super::{
    card_render::render_card_target,
    cards::CardParams,
    helpers::{parse_budget, render_result},
    limits::MAX_CARD_TARGETS,
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
            return state
                .with_read_compiler(|compiler| {
                    render_card_target(
                        state,
                        compiler,
                        &targets[0],
                        parse_budget(&params.budget),
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
        let budget = parse_budget(&params.budget);
        let mut cards = Vec::new();
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
                        Ok(card) => cards.push(card),
                        Err(error) => errors.push(json!({
                            "target": target,
                            "error": super::error::error_value(&error),
                        })),
                    }
                }
                Ok(())
            })
            .map_err(|err| anyhow::anyhow!(err))?;
        Ok(json!({
            "budget": params.budget,
            "cards": cards,
            "errors": errors,
            "total": cards.len(),
        }))
    })();
    render_result(result)
}
