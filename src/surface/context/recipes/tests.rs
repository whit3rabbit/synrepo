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
