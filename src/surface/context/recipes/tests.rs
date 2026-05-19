use crate::surface::context::compiler::compile_context_request;
use crate::surface::context::recipe::ContextRecipe;
use crate::surface::context::types::{
    ContextAskRequest, ContextBudget, ContextScope, ContextShape, GroundingOptions,
};

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

fn target_keys(request: &ContextAskRequest) -> Vec<String> {
    compile_context_request(request)
        .unwrap()
        .targets
        .into_iter()
        .map(|target| format!("{}:{}", target.kind, target.target))
        .collect()
}

#[test]
fn every_named_recipe_infers() {
    assert_eq!(
        ContextRecipe::infer("explain symbol alpha"),
        ContextRecipe::ExplainSymbol
    );
    assert_eq!(
        ContextRecipe::infer("trace call chain"),
        ContextRecipe::TraceCall
    );
    assert_eq!(
        ContextRecipe::infer("review module"),
        ContextRecipe::ReviewModule
    );
    assert_eq!(
        ContextRecipe::infer("security review"),
        ContextRecipe::SecurityReview
    );
    assert_eq!(
        ContextRecipe::infer("release readiness"),
        ContextRecipe::ReleaseReadiness
    );
    assert_eq!(
        ContextRecipe::infer("fix failing test"),
        ContextRecipe::FixTest
    );
}

#[test]
fn review_directory_uses_module_artifacts() {
    let mut req = request("review module");
    req.scope.paths = vec!["src/surface/mcp".into()];

    let keys = target_keys(&req);

    assert!(keys.contains(&"directory:src/surface/mcp".to_string()));
    assert!(keys.contains(&"public_api:src/surface/mcp".to_string()));
    assert!(keys.contains(&"entrypoints:src/surface/mcp".to_string()));
    assert!(!keys.contains(&"minimum_context:src/surface/mcp".to_string()));
}

#[test]
fn empty_scope_lifecycle_review_uses_entrypoints_and_probes() {
    let req = request(
        "Review likely memory/resource leaks and uncaught error handling risks in long-lived server, goroutine, PTY/process, database, and timer lifecycle code",
    );

    let keys = target_keys(&req);

    assert!(keys.contains(&"public_api:.".to_string()));
    assert!(keys.contains(&"entrypoints:.".to_string()));
    assert!(keys.contains(&"search:go func".to_string()));
    assert!(keys.contains(&"search:goroutine".to_string()));
    assert!(keys.contains(&"search:QueryContext".to_string()));
    assert!(keys.contains(&"search:panic".to_string()));
}

#[test]
fn ask_request_accepts_shorthand_fields() {
    let req: ContextAskRequest = serde_json::from_value(serde_json::json!({
        "ask": "Review codebase",
        "scope": "cmd/agent-to-api-dataset/main.go internal/provider internal/runtime",
        "shape": "findings; tests",
        "ground": "observed graph/source only",
        "budget": "normal"
    }))
    .unwrap();

    assert_eq!(
        req.scope.paths,
        vec![
            "cmd/agent-to-api-dataset/main.go",
            "internal/provider",
            "internal/runtime",
        ]
    );
    assert_eq!(req.shape.sections, vec!["findings", "tests"]);
    assert_eq!(
        req.ground.mode,
        crate::surface::context::GroundingMode::Required
    );
    assert!(!req.ground.allow_overlay);
    assert_eq!(req.budget.tier.as_deref(), Some("normal"));
}
