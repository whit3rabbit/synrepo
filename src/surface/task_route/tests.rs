use super::*;

#[test]
fn classifies_context_fast_path() {
    let route = classify_task_route("find where the CLI command is implemented", None);
    assert_eq!(route.intent, "context-search");
    assert_eq!(route.routing_strategy, "keyword_fallback");
    assert!(!route.llm_required);
    assert!(route
        .signals
        .contains(&SIGNAL_CONTEXT_FAST_PATH.to_string()));
}

#[test]
fn classifies_unsupported_semantic_transform_as_llm_required() {
    let route = classify_task_route("add TypeScript type annotations", Some("src/app.ts"));
    assert_eq!(route.intent, "llm-required");
    assert!(route.llm_required);
    assert!(route.edit_candidate.is_none());
}

#[test]
fn config_aware_route_falls_back_without_semantic_assets() {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config {
        enable_semantic_triage: true,
        semantic_model: "definitely-not-a-built-in-model".to_string(),
        ..crate::config::Config::default()
    };
    let route = classify_task_route_with_config(
        "convert all let bindings to constants",
        Some("src/app.ts"),
        &config,
        dir.path(),
    );

    assert_eq!(route.routing_strategy, "keyword_fallback");
    assert!(route.semantic_score.is_none());
}

#[test]
fn classifies_var_to_const_candidate() {
    let route = classify_task_route("convert var to const", Some("src/app.ts"));
    assert_eq!(route.intent, "var-to-const");
    assert!(!route.llm_required);
    assert_eq!(
        route
            .edit_candidate
            .as_ref()
            .map(|candidate| candidate.intent.as_str()),
        Some("var-to-const")
    );
}

#[test]
fn typescript_var_to_const_accepts_unreassigned_binding() {
    let result =
        typescript_var_to_const_eligibility("let value = 1;\nconsole.log(value);\n", false);
    assert!(result.eligible, "{result:?}");
    assert_eq!(result.binding.as_deref(), Some("value"));
}

#[test]
fn typescript_var_to_const_rejects_reassigned_binding() {
    let result = typescript_var_to_const_eligibility("let value = 1;\nvalue = 2;\n", false);
    assert!(!result.eligible);
    assert_eq!(result.binding.as_deref(), Some("value"));
}
