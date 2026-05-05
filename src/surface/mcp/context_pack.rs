use std::time::Instant;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::ids::{FileNodeId, NodeId};
use crate::surface::card::accounting::{estimate_tokens_bytes, ContextAccounting};
use crate::surface::card::{neighborhood, Budget, CardCompiler};

use super::compact::OutputMode;
use super::helpers::{parse_budget, render_result};
use super::SynrepoState;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContextPackParams {
    pub repo_root: Option<std::path::PathBuf>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
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

#[derive(Clone, Copy)]
struct ArtifactOptions {
    include_notes: bool,
    limit: usize,
    output_mode: OutputMode,
    budget_tokens: Option<usize>,
}

pub fn default_limit() -> usize {
    8
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
    let default_budget = parse_budget(&params.budget);
    let mut targets = params.targets;

    if targets.is_empty() {
        if let Some(goal) = params.goal.as_ref().filter(|g| !g.trim().is_empty()) {
            targets.push(ContextPackTarget {
                kind: "search".to_string(),
                target: goal.clone(),
                budget: Some("tiny".to_string()),
            });
        }
    }

    let mut artifacts = state
        .with_read_compiler(|compiler| {
            let mut artifacts = Vec::new();
            for target in targets.into_iter().take(params.limit) {
                let target_budget = target
                    .budget
                    .as_deref()
                    .map(parse_budget)
                    .unwrap_or(default_budget);
                let options = ArtifactOptions {
                    include_notes: params.include_notes,
                    limit: params.limit,
                    output_mode: params.output_mode,
                    budget_tokens: params.budget_tokens,
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

    let mut omitted = Vec::new();
    let truncation_applied =
        accounting::apply_pack_cap(&mut artifacts, &mut omitted, params.budget_tokens);
    let accountings = accounting::collect_artifact_accountings(&artifacts);
    accounting::record_pack_metrics(state, &accountings, start);

    let context_state =
        accounting::context_state(state, default_budget, &accountings, truncation_applied);
    let totals = json!({
        "artifact_count": artifacts.len(),
        "omitted_count": omitted.len(),
        "token_estimate": accountings.iter().map(|a| a.token_estimate).sum::<usize>(),
        "raw_file_token_estimate": accountings.iter().map(|a| a.raw_file_token_estimate).sum::<usize>(),
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

fn build_artifact(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &ContextPackTarget,
    budget: Budget,
    options: ArtifactOptions,
) -> Value {
    let result = match target.kind.as_str() {
        "file" => file_outline_artifact(
            state,
            compiler,
            &target.target,
            budget,
            options.include_notes,
        ),
        "symbol" => symbol_artifact(
            state,
            compiler,
            &target.target,
            budget,
            options.include_notes,
        ),
        "directory" => compiler
            .module_card(&target.target, budget)
            .and_then(|card| to_value(card).map(|value| artifact("module_card", target, value))),
        "minimum_context" => neighborhood::resolve_neighborhood(compiler, &target.target, budget)
            .and_then(|response| {
                to_value(response).map(|value| artifact("minimum_context", target, value))
            }),
        "test_surface" => compiler
            .test_surface_card(&target.target, budget)
            .and_then(|card| to_value(card).map(|value| artifact("test_surface", target, value))),
        "call_path" => call_path_artifact(compiler, target, budget),
        "search" => search::search_artifact(
            state,
            target,
            options.limit,
            options.output_mode,
            options.budget_tokens,
        ),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "unsupported context-pack target kind: {other}"
        ))),
    };

    result.unwrap_or_else(|err| error_artifact(target, err.to_string(), budget))
}

fn file_outline_artifact(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
    budget: Budget,
    include_notes: bool,
) -> crate::Result<Value> {
    let node_id = resolve_file_node(compiler, target)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("file target not found: {target}")))?;
    let content = compiler.with_reader(|graph| file_outline_content(graph, node_id, budget))?;
    let mut artifact = artifact_value("file_outline", "file", target, content);
    if include_notes {
        super::notes::attach_agent_notes(state, &mut artifact["content"], NodeId::File(node_id))?;
    }
    Ok(artifact)
}

fn symbol_artifact(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
    budget: Budget,
    include_notes: bool,
) -> crate::Result<Value> {
    let node_id = compiler
        .resolve_target(target)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("target not found: {target}")))?;
    let NodeId::Symbol(sym_id) = node_id else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "symbol target resolved to non-symbol: {target}"
        )));
    };
    let card = compiler.symbol_card(sym_id, budget)?;
    let mut content = to_value(card)?;
    if include_notes {
        super::notes::attach_agent_notes(state, &mut content, NodeId::Symbol(sym_id))?;
    }
    Ok(artifact_value("symbol_card", "symbol", target, content))
}

fn call_path_artifact(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &ContextPackTarget,
    budget: Budget,
) -> crate::Result<Value> {
    let node_id = compiler.resolve_target(&target.target)?.ok_or_else(|| {
        crate::Error::Other(anyhow::anyhow!("target not found: {}", target.target))
    })?;
    let NodeId::Symbol(sym_id) = node_id else {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "call_path target must resolve to a symbol"
        )));
    };
    let card = compiler.call_path_card(sym_id, budget)?;
    Ok(artifact("call_path", target, to_value(card)?))
}

fn resolve_file_node(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
) -> crate::Result<Option<FileNodeId>> {
    if let Some(file) = compiler.reader().file_by_path(target)? {
        return Ok(Some(file.id));
    }
    Ok(match compiler.resolve_target(target)? {
        Some(NodeId::File(id)) => Some(id),
        _ => None,
    })
}

fn maybe_test_surface_for(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &ContextPackTarget,
    budget: Budget,
) -> Option<Value> {
    let scope = match target.kind.as_str() {
        "file" | "directory" => target.target.clone(),
        "symbol" | "call_path" | "minimum_context" => symbol_file_path(compiler, &target.target)?,
        _ => return None,
    };
    let t = ContextPackTarget {
        kind: "test_surface".to_string(),
        target: scope,
        budget: None,
    };
    compiler
        .test_surface_card(&t.target, budget)
        .ok()
        .and_then(|card| serde_json::to_value(card).ok())
        .map(|content| artifact("test_surface", &t, content))
}

fn symbol_file_path(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
) -> Option<String> {
    let Some(NodeId::Symbol(sym_id)) = compiler.resolve_target(target).ok().flatten() else {
        return None;
    };
    let symbol = compiler.reader().get_symbol(sym_id).ok().flatten()?;
    compiler
        .reader()
        .get_file(symbol.file_id)
        .ok()
        .flatten()
        .map(|file| file.path)
}

fn artifact(kind: &str, target: &ContextPackTarget, content: Value) -> Value {
    artifact_value(kind, &target.kind, &target.target, content)
}

fn artifact_value(artifact_type: &str, kind: &str, target: &str, mut content: Value) -> Value {
    if content.get("context_accounting").is_none() {
        attach_estimated_accounting(&mut content, Budget::Tiny);
    }
    let accounting = content
        .get("context_accounting")
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "artifact_type": artifact_type,
        "target_kind": kind,
        "target": target,
        "status": "ok",
        "content": content,
        "context_accounting": accounting,
    })
}

fn error_artifact(target: &ContextPackTarget, message: String, budget: Budget) -> Value {
    let content = json!({ "error": message });
    let accounting = estimated_accounting(&content, budget);
    json!({
        "artifact_type": target.kind,
        "target_kind": target.kind,
        "target": target.target,
        "status": "error",
        "content": content,
        "context_accounting": accounting,
    })
}

fn attach_estimated_accounting(content: &mut Value, budget: Budget) {
    let accounting = estimated_accounting(content, budget);
    if let Some(obj) = content.as_object_mut() {
        obj.insert(
            "context_accounting".to_string(),
            serde_json::to_value(accounting).unwrap_or(Value::Null),
        );
    }
}

fn estimated_accounting(content: &Value, budget: Budget) -> ContextAccounting {
    let bytes = serde_json::to_vec(content).map(|v| v.len()).unwrap_or(4);
    ContextAccounting::new(budget, estimate_tokens_bytes(bytes), 0, Vec::new())
}

fn to_value<T: serde::Serialize>(value: T) -> crate::Result<Value> {
    serde_json::to_value(value).map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))
}

mod accounting;
mod outline;
mod resource;
mod search;
#[cfg(test)]
mod tests;
use outline::file_outline_content;
pub use resource::read_resource;
