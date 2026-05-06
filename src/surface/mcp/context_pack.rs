use std::time::Instant;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::compact::OutputMode;
use super::helpers::{parse_budget, render_result};
use super::limits::DEFAULT_CONTEXT_PACK_LIMIT;
use super::SynrepoState;
use artifacts::{build_artifact, maybe_test_surface_for, ArtifactOptions};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContextPackParams {
    pub repo_root: Option<std::path::PathBuf>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    #[schemars(description = "Structured targets: [{kind,target,budget?}].")]
    pub targets: Vec<ContextPackTarget>,
    #[serde(default = "super::cards::default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
    #[serde(default)]
    pub output_mode: OutputMode,
    #[serde(default)]
    pub include_tests: bool,
    #[serde(default)]
    pub include_notes: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ContextPackTarget {
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub budget: Option<String>,
}

pub fn default_limit() -> usize {
    DEFAULT_CONTEXT_PACK_LIMIT
}

pub fn handle_context_pack(state: &SynrepoState, params: ContextPackParams) -> String {
    render_result(build_context_pack(state, params))
}

pub fn handle_file_outline_resource(state: &SynrepoState, path: String, budget: String) -> String {
    handle_context_pack(
        state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![ContextPackTarget {
                kind: "file".to_string(),
                target: path,
                budget: Some(budget),
            }],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 1,
        },
    )
}

pub fn build_context_pack(
    state: &SynrepoState,
    params: ContextPackParams,
) -> anyhow::Result<Value> {
    let start = Instant::now();
    let default_budget = parse_budget(&params.budget)?;
    let prepared = targeting::prepare_targets(
        params.goal.as_ref(),
        params.targets,
        params.limit,
        params.budget_tokens,
    )?;
    let effective_limit = prepared.limit;
    let budget_tokens = prepared.budget_tokens;
    let mut omitted = prepared.omitted;
    let targets = prepared.targets;
    let mut artifacts = state
        .with_read_compiler(|compiler| {
            let mut artifacts = Vec::new();
            for target in targets.into_iter().take(effective_limit) {
                let target_budget = target
                    .budget
                    .as_deref()
                    .map(parse_budget)
                    .transpose()?
                    .unwrap_or(default_budget);
                let options = ArtifactOptions {
                    include_notes: params.include_notes,
                    limit: effective_limit,
                    output_mode: params.output_mode,
                    budget_tokens,
                };
                artifacts.push(build_artifact(
                    state,
                    compiler,
                    &target,
                    target_budget,
                    options,
                ));
                if params.include_tests {
                    if let Some(extra) = maybe_test_surface_for(compiler, &target, default_budget) {
                        artifacts.push(extra);
                    }
                }
            }
            Ok(artifacts)
        })
        .map_err(|e| anyhow::anyhow!(e))?;

    let truncation_applied =
        accounting::apply_pack_cap(&mut artifacts, &mut omitted, budget_tokens);
    let accountings = accounting::collect_artifact_accountings(&artifacts);
    let token_estimate = accountings.iter().map(|a| a.token_estimate).sum::<usize>();
    accounting::record_pack_metrics(state, &accountings, start, token_estimate);

    let context_state =
        accounting::context_state(state, default_budget, &accountings, truncation_applied);
    let totals = json!({
        "artifact_count": artifacts.len(),
        "omitted_count": omitted.len(),
        "token_estimate": token_estimate,
        "raw_file_token_estimate": accountings.iter().map(|a| a.raw_file_token_estimate).sum::<usize>(),
        "limit": effective_limit,
        "token_cap": budget_tokens,
    });

    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "goal": params.goal,
        "context_state": context_state,
        "artifacts": artifacts,
        "omitted": omitted,
        "totals": totals,
    }))
}

mod accounting;
mod artifacts;
mod outline;
mod resource;
mod search;
mod targeting;
#[cfg(test)]
mod tests;
pub use resource::read_resource;
