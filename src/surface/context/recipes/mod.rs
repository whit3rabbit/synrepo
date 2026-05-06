use std::path::Path;

use super::recipe::ContextRecipe;
use super::types::{ContextAskRequest, ContextTarget};

mod explain_symbol;
mod fix_test;
mod release_readiness;
mod review_module;
mod security_review;
#[cfg(test)]
mod tests;
mod trace_call;

pub(crate) fn plan_targets(
    request: &ContextAskRequest,
    recipe: ContextRecipe,
    budget_tier: &str,
    notes: &mut Vec<String>,
) -> Vec<ContextTarget> {
    let mut targets = Vec::new();
    match recipe {
        ContextRecipe::ExplainSymbol => {
            explain_symbol::plan(&mut targets, request, budget_tier);
        }
        ContextRecipe::TraceCall => {
            trace_call::plan(&mut targets, request, budget_tier);
        }
        ContextRecipe::ReviewModule => {
            review_module::plan(&mut targets, request, budget_tier);
        }
        ContextRecipe::SecurityReview => {
            security_review::plan(&mut targets, request, budget_tier);
        }
        ContextRecipe::ReleaseReadiness => {
            release_readiness::plan(&mut targets, request, budget_tier, notes);
        }
        ContextRecipe::FixTest => {
            fix_test::plan(&mut targets, request, budget_tier);
        }
        ContextRecipe::General => {
            add_general_targets(&mut targets, request, budget_tier);
        }
    }
    targets
}

fn add_general_targets(
    targets: &mut Vec<ContextTarget>,
    request: &ContextAskRequest,
    budget: &str,
) {
    add_basic_scopes(targets, request, budget);
    if targets.is_empty() {
        push_unique(targets, "search", request.ask.trim(), Some(budget));
    }
}

pub(super) fn add_basic_scopes(
    targets: &mut Vec<ContextTarget>,
    request: &ContextAskRequest,
    budget: &str,
) {
    for path in scoped_paths(request) {
        push_unique(targets, path_target_kind(path), path, Some(budget));
    }
    for symbol in scoped_symbols(request) {
        push_unique(targets, "symbol", symbol, Some(budget));
    }
}

pub(crate) fn push_unique(
    targets: &mut Vec<ContextTarget>,
    kind: &str,
    target: &str,
    budget: Option<&str>,
) {
    if targets
        .iter()
        .any(|existing| existing.kind == kind && existing.target == target)
    {
        return;
    }
    targets.push(ContextTarget {
        kind: kind.to_string(),
        target: target.to_string(),
        budget: budget.map(str::to_string),
    });
}

pub(super) fn scoped_paths(request: &ContextAskRequest) -> impl Iterator<Item = &str> {
    request
        .scope
        .paths
        .iter()
        .map(String::as_str)
        .filter(|path| !path.trim().is_empty())
}

pub(super) fn scoped_symbols(request: &ContextAskRequest) -> impl Iterator<Item = &str> {
    request
        .scope
        .symbols
        .iter()
        .map(String::as_str)
        .filter(|symbol| !symbol.trim().is_empty())
}

pub(super) fn path_target_kind(path: &str) -> &'static str {
    if is_file_path(path) {
        "file"
    } else {
        "directory"
    }
}

pub(super) fn is_file_path(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    Path::new(trimmed).extension().is_some()
}

pub(super) fn push_searches(
    targets: &mut Vec<ContextTarget>,
    queries: impl IntoIterator<Item = impl AsRef<str>>,
) {
    for query in queries {
        let query = query.as_ref();
        if !query.trim().is_empty() {
            push_unique(targets, "search", query, Some("tiny"));
        }
    }
}
