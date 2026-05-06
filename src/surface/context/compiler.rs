use std::path::Path;

use serde::Serialize;

#[cfg(test)]
use std::collections::BTreeSet;

use super::recipe::ContextRecipe;
use super::types::{ContextAskRequest, ContextTarget, GroundingMode, GroundingOptions};

/// Deterministic plan for a high-level task-context request.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CompiledContextPlan {
    /// Built-in recipe inferred from the ask text.
    pub recipe: ContextRecipe,
    /// Card budget tier passed to rendered context-pack targets.
    pub budget_tier: String,
    /// Numeric token cap for the resulting context packet.
    pub budget_tokens: usize,
    /// Maximum number of planned targets to render.
    pub limit: usize,
    /// Whether test-surface artifacts should be attached where possible.
    pub include_tests: bool,
    /// Whether advisory overlay notes may be included.
    pub include_notes: bool,
    /// Existing context-pack targets selected by the recipe.
    pub targets: Vec<ContextTarget>,
    /// Human-readable notes about skipped or downgraded context.
    pub omitted_context_notes: Vec<String>,
    /// Recommended lower-level drill-down tools after the packet.
    pub next_best_tools: Vec<String>,
}

/// Compile a high-level ask into a deterministic context-pack plan.
pub fn compile_context_request(request: &ContextAskRequest) -> anyhow::Result<CompiledContextPlan> {
    let ask = request.ask.trim();
    if ask.is_empty() {
        anyhow::bail!("synrepo_ask requires a non-empty ask");
    }

    let recipe = ContextRecipe::infer(ask);
    let budget_tier = request
        .budget
        .tier
        .clone()
        .unwrap_or_else(|| recipe.default_budget_tier().to_string());
    let include_tests = wants_tests(recipe, &request.shape.sections);
    let mut targets = Vec::new();
    add_scoped_targets(&mut targets, request, recipe, &budget_tier);
    add_recipe_search_targets(&mut targets, ask, recipe, &budget_tier);
    if targets.is_empty() {
        push_unique(&mut targets, "search", ask, Some("tiny"));
    }

    let limit = target_limit(request).min(targets.len().max(1));
    let omitted_context_notes = omitted_notes(request, targets.len(), limit);

    Ok(CompiledContextPlan {
        recipe,
        budget_tier,
        budget_tokens: request.budget.max_tokens.max(1),
        limit,
        include_tests,
        include_notes: request.ground.allow_overlay,
        targets,
        omitted_context_notes,
        next_best_tools: recipe.next_tools(),
    })
}

fn add_scoped_targets(
    targets: &mut Vec<ContextTarget>,
    request: &ContextAskRequest,
    recipe: ContextRecipe,
    budget_tier: &str,
) {
    for path in request.scope.paths.iter().filter(|p| !p.trim().is_empty()) {
        let kind = path_target_kind(path);
        push_unique(targets, kind, path, Some(budget_tier));
        if matches!(
            recipe,
            ContextRecipe::ReviewModule | ContextRecipe::ReleaseReadiness
        ) {
            push_unique(targets, "minimum_context", path, Some("tiny"));
        }
    }

    for symbol in request
        .scope
        .symbols
        .iter()
        .filter(|s| !s.trim().is_empty())
    {
        match recipe {
            ContextRecipe::TraceCall => {
                push_unique(targets, "call_path", symbol, Some(budget_tier));
                push_unique(targets, "minimum_context", symbol, Some("normal"));
            }
            ContextRecipe::ExplainSymbol | ContextRecipe::FixTest => {
                push_unique(targets, "symbol", symbol, Some(budget_tier));
                push_unique(targets, "minimum_context", symbol, Some("tiny"));
            }
            _ => {
                push_unique(targets, "symbol", symbol, Some(budget_tier));
            }
        }
    }
}

fn add_recipe_search_targets(
    targets: &mut Vec<ContextTarget>,
    ask: &str,
    recipe: ContextRecipe,
    budget_tier: &str,
) {
    match recipe {
        ContextRecipe::SecurityReview => {
            for query in ["unsafe", "Command::new", "std::fs", "auth", ask] {
                push_unique(targets, "search", query, Some("tiny"));
            }
        }
        ContextRecipe::ReleaseReadiness => {
            for query in ["TODO", "FIXME", "panic!", "unwrap()", ask] {
                push_unique(targets, "search", query, Some("tiny"));
            }
        }
        ContextRecipe::FixTest => {
            for query in ["#[test]", "assert", ask] {
                push_unique(targets, "search", query, Some("tiny"));
            }
        }
        ContextRecipe::TraceCall if targets.is_empty() => {
            push_unique(targets, "search", ask, Some("tiny"));
        }
        ContextRecipe::ReviewModule | ContextRecipe::General if targets.is_empty() => {
            push_unique(targets, "search", ask, Some(budget_tier));
        }
        ContextRecipe::ExplainSymbol if targets.is_empty() => {
            push_unique(targets, "search", ask, Some("tiny"));
        }
        _ => {}
    }
}

fn wants_tests(recipe: ContextRecipe, sections: &[String]) -> bool {
    matches!(
        recipe,
        ContextRecipe::FixTest | ContextRecipe::ReleaseReadiness
    ) || sections
        .iter()
        .any(|section| section.eq_ignore_ascii_case("tests"))
}

fn path_target_kind(path: &str) -> &'static str {
    let trimmed = path.trim_end_matches('/');
    if Path::new(trimmed).extension().is_some() {
        "file"
    } else {
        "directory"
    }
}

fn target_limit(request: &ContextAskRequest) -> usize {
    let file_limit = request.budget.max_files.max(1);
    let symbol_limit = request.budget.max_symbols.max(1);
    file_limit.saturating_add(symbol_limit).clamp(1, 20)
}

fn omitted_notes(request: &ContextAskRequest, target_count: usize, limit: usize) -> Vec<String> {
    let mut notes = Vec::new();
    if !request.ground.allow_overlay {
        notes.push("overlay-backed commentary and agent notes were excluded".to_string());
    }
    if request.ground.mode == GroundingMode::Off {
        notes.push("citation grounding was disabled by request".to_string());
    }
    if target_count > limit {
        notes.push(format!(
            "{} planned target(s) omitted by the context budget limit",
            target_count - limit
        ));
    }
    if let Some(change_set) = request.scope.change_set.as_deref() {
        notes.push(format!(
            "change_set={change_set} is advisory in phase 2 and does not mutate scope"
        ));
    }
    if request.budget.freshness.as_deref() == Some("current") {
        notes.push("packet was compiled from the current graph/index snapshot".to_string());
    }
    notes
}

fn push_unique(targets: &mut Vec<ContextTarget>, kind: &str, target: &str, budget: Option<&str>) {
    let key = (kind, target);
    if targets
        .iter()
        .any(|existing| existing.kind == key.0 && existing.target == key.1)
    {
        return;
    }
    targets.push(ContextTarget {
        kind: kind.to_string(),
        target: target.to_string(),
        budget: budget.map(str::to_string),
    });
}

#[cfg(test)]
fn planned_target_keys(plan: &CompiledContextPlan) -> BTreeSet<String> {
    plan.targets
        .iter()
        .map(|target| format!("{}:{}", target.kind, target.target))
        .collect()
}

/// Report whether the rendered packet satisfied the requested grounding mode.
pub fn grounding_status(ground: &GroundingOptions, evidence_count: usize) -> &'static str {
    match (ground.mode, evidence_count) {
        (GroundingMode::Off, _) => "disabled",
        (GroundingMode::Required, 0) => "insufficient",
        (_, 0) => "missing",
        _ => "grounded",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::context::types::{ContextBudget, ContextScope, ContextShape};

    fn request(ask: &str) -> ContextAskRequest {
        ContextAskRequest {
            repo_root: None,
            ask: ask.to_string(),
            scope: ContextScope::default(),
            shape: ContextShape::default(),
            ground: GroundingOptions::default(),
            budget: ContextBudget::default(),
        }
    }

    #[test]
    fn scoped_review_prefers_directory_artifacts_and_tests_when_requested() {
        let mut req = request("review this module");
        req.scope.paths = vec!["src/surface/mcp".into()];
        req.shape.sections = vec!["findings".into(), "tests".into()];

        let plan = compile_context_request(&req).unwrap();
        let keys = planned_target_keys(&plan);

        assert_eq!(plan.recipe, ContextRecipe::ReviewModule);
        assert!(plan.include_tests);
        assert!(keys.contains("directory:src/surface/mcp"));
        assert!(keys.contains("minimum_context:src/surface/mcp"));
    }

    #[test]
    fn trace_symbol_adds_call_path_before_minimum_context() {
        let mut req = request("trace call chain");
        req.scope.symbols = vec!["synrepo::main".into()];

        let plan = compile_context_request(&req).unwrap();

        assert_eq!(plan.recipe, ContextRecipe::TraceCall);
        assert_eq!(plan.targets[0].kind, "call_path");
        assert_eq!(plan.targets[1].kind, "minimum_context");
    }

    #[test]
    fn empty_scope_falls_back_to_search() {
        let plan = compile_context_request(&request("where is bootstrap handled")).unwrap();

        assert!(plan
            .targets
            .iter()
            .any(|target| target.kind == "search" && target.target.contains("bootstrap")));
    }
}
