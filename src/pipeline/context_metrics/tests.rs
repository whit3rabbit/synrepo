use super::*;

#[test]
fn record_card_updates_token_and_budget_totals() {
    let accounting = ContextAccounting::new(Budget::Tiny, 100, 1_000, vec!["hash".to_string()]);
    let mut metrics = ContextMetrics::default();
    metrics.record_card(&accounting, 25);

    assert_eq!(metrics.cards_served_total, 1);
    assert_eq!(metrics.card_tokens_total, 100);
    assert_eq!(metrics.raw_file_tokens_total, 1_000);
    assert_eq!(metrics.estimated_tokens_saved_total, 900);
    assert_eq!(metrics.budget_tier_usage.get("tiny"), Some(&1));
    assert_eq!(metrics.context_query_latency_ms_avg(), 25.0);
}

#[test]
fn record_workflow_call_increments_per_tool() {
    let mut metrics = ContextMetrics::default();
    metrics.record_workflow_call("orient");
    metrics.record_workflow_call("orient");
    metrics.record_workflow_call("minimum_context");

    assert_eq!(metrics.workflow_calls_total.get("orient"), Some(&2));
    assert_eq!(
        metrics.workflow_calls_total.get("minimum_context"),
        Some(&1)
    );
}

#[test]
fn record_mcp_tool_result_tracks_calls_errors_and_saved_context() {
    let mut metrics = ContextMetrics::default();
    metrics.record_mcp_tool_result("synrepo_note_add", false, Some("note_add"));
    metrics.record_mcp_tool_result("synrepo_search", true, None);

    assert_eq!(metrics.mcp_requests_total, 2);
    assert_eq!(
        metrics.mcp_tool_calls_total.get("synrepo_note_add"),
        Some(&1)
    );
    assert_eq!(
        metrics.mcp_tool_errors_total.get("synrepo_search"),
        Some(&1)
    );
    assert_eq!(metrics.saved_context_writes_total.get("note_add"), Some(&1));
}

#[test]
fn record_mcp_resource_read_counts_as_mcp_request() {
    let mut metrics = ContextMetrics::default();
    metrics.record_mcp_resource_read();

    assert_eq!(metrics.mcp_requests_total, 1);
    assert_eq!(metrics.mcp_resource_reads_total, 1);
}

#[test]
fn mcp_metrics_default_when_loading_older_json() {
    let old_shape = serde_json::json!({
        "cards_served_total": 1,
        "card_tokens_total": 10,
        "raw_file_tokens_total": 100,
        "estimated_tokens_saved_total": 90,
        "budget_tier_usage": {},
        "truncation_applied_total": 0,
        "stale_responses_total": 0,
        "test_surface_hits_total": 0,
        "changed_files_total": 0,
        "context_query_latency_ms_total": 5,
        "context_query_latency_samples": 1,
        "workflow_calls_total": {}
    });

    let metrics: ContextMetrics = serde_json::from_value(old_shape).unwrap();

    assert_eq!(metrics.mcp_requests_total, 0);
    assert!(metrics.mcp_tool_calls_total.is_empty());
    assert!(metrics.saved_context_writes_total.is_empty());
}
