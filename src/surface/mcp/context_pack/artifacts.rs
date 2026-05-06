use serde_json::{json, Value};

use crate::core::ids::{FileNodeId, NodeId};
use crate::pipeline::recent_activity::{
    read_recent_activity, RecentActivityKind, RecentActivityQuery,
};
use crate::surface::card::accounting::{estimate_tokens_bytes, ContextAccounting};
use crate::surface::card::{neighborhood, Budget, CardCompiler};

use super::super::compact::OutputMode;
use super::super::limits::{
    bounded_limit_value, DEFAULT_FINDINGS_LIMIT, DEFAULT_NOTES_LIMIT, MAX_FINDINGS_LIMIT,
    MAX_NOTES_LIMIT,
};
use super::super::SynrepoState;
use super::ContextPackTarget;

#[derive(Clone, Copy)]
pub(super) struct ArtifactOptions {
    pub include_notes: bool,
    pub limit: usize,
    pub output_mode: OutputMode,
    pub budget_tokens: Option<usize>,
}

pub(super) fn build_artifact(
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
        "entrypoints" => entrypoints_artifact(compiler, target, budget),
        "public_api" => compiler
            .public_api_card(&target.target, budget)
            .and_then(|card| to_value(card).map(|value| artifact("public_api", target, value))),
        "change_risk" => change_risk_artifact(compiler, target, budget),
        "findings" => findings_artifact(state, target, options.limit),
        "recent_activity" => recent_activity_artifact(state, target, options.limit),
        "minimum_context" => neighborhood::resolve_neighborhood(compiler, &target.target, budget)
            .and_then(|response| {
                to_value(response).map(|value| artifact("minimum_context", target, value))
            }),
        "test_surface" => compiler
            .test_surface_card(&target.target, budget)
            .and_then(|card| to_value(card).map(|value| artifact("test_surface", target, value))),
        "call_path" => call_path_artifact(compiler, target, budget),
        "search" => super::search::search_artifact(
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
    let content = compiler
        .with_reader(|graph| super::outline::file_outline_content(graph, node_id, budget))?;
    let mut artifact = artifact_value("file_outline", "file", target, content);
    if include_notes {
        super::super::notes::attach_agent_notes(
            state,
            &mut artifact["content"],
            NodeId::File(node_id),
        )?;
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
        super::super::notes::attach_agent_notes(state, &mut content, NodeId::Symbol(sym_id))?;
    }
    Ok(artifact_value("symbol_card", "symbol", target, content))
}

fn entrypoints_artifact(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &ContextPackTarget,
    budget: Budget,
) -> crate::Result<Value> {
    let scope = if target.target == "." {
        None
    } else {
        Some(target.target.as_str())
    };
    let card = compiler.entry_point_card(scope, budget)?;
    Ok(artifact("entrypoints", target, to_value(card)?))
}

fn change_risk_artifact(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &ContextPackTarget,
    budget: Budget,
) -> crate::Result<Value> {
    let node_id = compiler.resolve_target(&target.target)?.ok_or_else(|| {
        crate::Error::Other(anyhow::anyhow!("target not found: {}", target.target))
    })?;
    let card = compiler.change_risk_card(node_id, budget)?;
    Ok(artifact("change_risk", target, to_value(card)?))
}

fn findings_artifact(
    state: &SynrepoState,
    target: &ContextPackTarget,
    limit: usize,
) -> crate::Result<Value> {
    let node_id = if target.target == "all" {
        None
    } else {
        Some(target.target.clone())
    };
    let capped = bounded_limit_value(limit, DEFAULT_FINDINGS_LIMIT, MAX_FINDINGS_LIMIT) as u32;
    let mut content =
        super::super::findings::render_findings(&state.repo_root, node_id, None, None, capped)
            .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?;
    if let Some(obj) = content.as_object_mut() {
        obj.insert("source_store".to_string(), json!("overlay"));
    }
    Ok(artifact("findings", target, content))
}

fn recent_activity_artifact(
    state: &SynrepoState,
    target: &ContextPackTarget,
    limit: usize,
) -> crate::Result<Value> {
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    let kinds = if target.target == "release_readiness" {
        Some(vec![
            RecentActivityKind::Reconcile,
            RecentActivityKind::Repair,
            RecentActivityKind::CrossLink,
            RecentActivityKind::OverlayRefresh,
            RecentActivityKind::Hotspot,
        ])
    } else {
        None
    };
    let query = RecentActivityQuery {
        kinds,
        limit: bounded_limit_value(limit, DEFAULT_NOTES_LIMIT, MAX_NOTES_LIMIT),
        since: None,
    };
    let entries = read_recent_activity(&synrepo_dir, &state.repo_root, &state.config, query)
        .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?;
    let content = json!({
        "activity": entries,
        "source_store": "operations",
    });
    Ok(artifact("recent_activity", target, content))
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

pub(super) fn maybe_test_surface_for(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &ContextPackTarget,
    budget: Budget,
) -> Option<Value> {
    let scope = match target.kind.as_str() {
        "file" | "directory" => target.target.clone(),
        "symbol" | "call_path" | "minimum_context" | "change_risk" => {
            symbol_file_path(compiler, &target.target)?
        }
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
        return Some(target.to_string());
    };
    let symbol = compiler.reader().get_symbol(sym_id).ok().flatten()?;
    compiler
        .reader()
        .get_file(symbol.file_id)
        .ok()
        .flatten()
        .map(|file| file.path)
}

pub(super) fn artifact(kind: &str, target: &ContextPackTarget, content: Value) -> Value {
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
    let error = error_object(&message);
    let accounting = estimated_accounting(&error, budget);
    json!({
        "artifact_type": "error",
        "target_kind": target.kind,
        "target": target.target,
        "status": "error",
        "severity": "warning",
        "error": error,
        "content": Value::Null,
        "context_accounting": accounting,
    })
}

fn error_object(message: &str) -> Value {
    let error = anyhow::anyhow!(message.to_string());
    let code = super::super::error::classify_error(&error);
    json!({
        "code": code.as_str(),
        "message": message,
        "retryable": matches!(
            code,
            super::super::error::ErrorCode::RateLimited
                | super::super::error::ErrorCode::Locked
                | super::super::error::ErrorCode::Busy
                | super::super::error::ErrorCode::Timeout
        ),
    })
}

pub(super) fn attach_estimated_accounting(content: &mut Value, budget: Budget) {
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
