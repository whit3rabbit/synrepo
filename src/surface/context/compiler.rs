use serde::Serialize;

#[cfg(test)]
use std::collections::BTreeSet;

use super::recipe::ContextRecipe;
use super::recipes;
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
    let mut recipe_notes = Vec::new();
    let mut targets = recipes::plan_targets(request, recipe, &budget_tier, &mut recipe_notes);
    if targets.is_empty() {
        recipes::push_unique(&mut targets, "search", ask, Some("tiny"));
    }

    let limit = target_limit(request).min(targets.len().max(1));
    let mut omitted_context_notes = omitted_notes(request, targets.len(), limit);
    omitted_context_notes.append(&mut recipe_notes);

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

fn wants_tests(recipe: ContextRecipe, sections: &[String]) -> bool {
    matches!(
        recipe,
        ContextRecipe::FixTest | ContextRecipe::ReleaseReadiness
    ) || sections
        .iter()
        .any(|section| section.eq_ignore_ascii_case("tests"))
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
        assert!(keys.contains("public_api:src/surface/mcp"));
        assert!(keys.contains("entrypoints:src/surface/mcp"));
        assert!(!keys.contains("minimum_context:src/surface/mcp"));
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
    fn explain_symbol_adds_symbol_context_and_call_path() {
        let mut req = request("explain this symbol");
        req.scope.symbols = vec!["alpha".into()];

        let plan = compile_context_request(&req).unwrap();
        let keys = planned_target_keys(&plan);

        assert_eq!(plan.recipe, ContextRecipe::ExplainSymbol);
        assert!(keys.contains("symbol:alpha"));
        assert!(keys.contains("minimum_context:alpha"));
        assert!(keys.contains("call_path:alpha"));
    }

    #[test]
    fn security_review_adds_entrypoints_and_risky_searches() {
        let plan =
            compile_context_request(&request("security review for command injection")).unwrap();
        let keys = planned_target_keys(&plan);

        assert_eq!(plan.recipe, ContextRecipe::SecurityReview);
        assert!(keys.contains("entrypoints:."));
        assert!(keys.contains("search:Command::new"));
        assert!(keys.contains("search:TcpStream"));
    }

    #[test]
    fn release_readiness_respects_overlay_gate() {
        let plan = compile_context_request(&request("release readiness")).unwrap();
        let keys = planned_target_keys(&plan);

        assert_eq!(plan.recipe, ContextRecipe::ReleaseReadiness);
        assert!(keys.contains("recent_activity:release_readiness"));
        assert!(!keys.contains("findings:all"));
        assert!(plan
            .omitted_context_notes
            .iter()
            .any(|note| note.contains("findings were excluded")));
    }

    #[test]
    fn release_readiness_includes_findings_when_overlay_allowed() {
        let mut req = request("release readiness");
        req.ground.allow_overlay = true;

        let plan = compile_context_request(&req).unwrap();
        let keys = planned_target_keys(&plan);

        assert!(keys.contains("findings:all"));
        assert!(keys.contains("recent_activity:release_readiness"));
    }

    #[test]
    fn fix_test_adds_test_surface_and_target_context() {
        let mut req = request("fix failing test");
        req.scope.paths = vec!["src/lib.rs".into()];

        let plan = compile_context_request(&req).unwrap();
        let keys = planned_target_keys(&plan);

        assert_eq!(plan.recipe, ContextRecipe::FixTest);
        assert!(plan.include_tests);
        assert!(keys.contains("test_surface:src/lib.rs"));
        assert!(keys.contains("file:src/lib.rs"));
        assert!(keys.contains("minimum_context:src/lib.rs"));
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
