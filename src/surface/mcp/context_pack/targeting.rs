use serde_json::{json, Value};

use super::ContextPackTarget;
use crate::surface::mcp::limits::{
    bounded_limit_value, DEFAULT_CONTEXT_PACK_LIMIT, DEFAULT_CONTEXT_PACK_TOKEN_CAP,
    MAX_CONTEXT_PACK_TARGETS, MAX_RESPONSE_TOKEN_CAP,
};

pub(super) struct PreparedTargets {
    pub targets: Vec<ContextPackTarget>,
    pub limit: usize,
    pub budget_tokens: Option<usize>,
    pub omitted: Vec<Value>,
}

pub(super) fn prepare_targets(
    goal: Option<&String>,
    mut targets: Vec<ContextPackTarget>,
    requested_limit: usize,
    requested_budget_tokens: Option<usize>,
) -> anyhow::Result<PreparedTargets> {
    let limit = bounded_limit_value(
        requested_limit,
        DEFAULT_CONTEXT_PACK_LIMIT,
        MAX_CONTEXT_PACK_TARGETS,
    );
    let budget_tokens = Some(
        requested_budget_tokens
            .unwrap_or(DEFAULT_CONTEXT_PACK_TOKEN_CAP)
            .clamp(1, MAX_RESPONSE_TOKEN_CAP),
    );
    if targets.is_empty() {
        if let Some(goal) = goal.filter(|goal| !goal.trim().is_empty()) {
            targets.push(ContextPackTarget {
                kind: "search".to_string(),
                target: goal.clone(),
                budget: Some("tiny".to_string()),
            });
        } else {
            return Err(crate::surface::mcp::error::McpError::invalid_parameter(
                "synrepo_context_pack requires explicit targets or a non-empty goal",
            )
            .into());
        }
    }
    let omitted = targets
        .iter()
        .skip(limit)
        .map(|target| {
            json!({
                "target": target.target.clone(),
                "artifact_type": target.kind.clone(),
                "reason": "limit_reached",
            })
        })
        .collect();
    Ok(PreparedTargets {
        targets,
        limit,
        budget_tokens,
        omitted,
    })
}
