use super::*;

#[test]
fn prometheus_output_matches_golden_string() {
    let mut metrics = ContextMetrics::default();
    metrics.cards_served_total = 3;
    metrics.card_tokens_total = 240;
    metrics.raw_file_tokens_total = 2_400;
    metrics.estimated_tokens_saved_total = 2_160;
    metrics.stale_responses_total = 1;
    metrics.truncation_applied_total = 0;
    metrics.test_surface_hits_total = 2;
    metrics.changed_files_total = 4;
    metrics.mcp_requests_total = 4;
    metrics.mcp_resource_reads_total = 1;
    metrics.compact_outputs_total = 2;
    metrics.compact_returned_tokens_total = 80;
    metrics.compact_original_tokens_total = 400;
    metrics.compact_estimated_tokens_saved_total = 320;
    metrics.compact_omitted_items_total = 5;
    metrics.compact_truncation_applied_total = 1;
    metrics.route_classifications_total = 6;
    metrics.context_fast_path_signals_total = 3;
    metrics.deterministic_edit_candidates_total = 2;
    metrics.deterministic_edit_candidate_signals_total = 1;
    metrics.llm_not_required_signals_total = 3;
    metrics.anchored_edit_accepted_total = 2;
    metrics.anchored_edit_rejected_total = 1;
    metrics.cross_link_generation_total = 7;
    metrics.cross_link_promoted_total = 2;
    metrics.commentary_refresh_total = 5;
    metrics.commentary_refresh_errors_total = 1;
    metrics.estimated_llm_calls_avoided_total = 5;
    metrics.budget_tier_usage.insert("tiny".to_string(), 2);
    metrics.budget_tier_usage.insert("normal".to_string(), 1);
    metrics
        .mcp_tool_calls_total
        .insert("synrepo_search".to_string(), 2);
    metrics
        .mcp_tool_errors_total
        .insert("synrepo_search".to_string(), 1);
    metrics
        .saved_context_writes_total
        .insert("note_add".to_string(), 1);
    metrics.workflow_calls_total.insert("orient".to_string(), 2);
    metrics.workflow_calls_total.insert("find".to_string(), 1);

    let expected = "\
# HELP synrepo_cards_served_total Observed: total number of card-shaped responses served.\n\
# TYPE synrepo_cards_served_total counter\n\
synrepo_cards_served_total 3\n\
# HELP synrepo_card_tokens_total Estimated: sum of estimated tokens in served cards.\n\
# TYPE synrepo_card_tokens_total counter\n\
synrepo_card_tokens_total 240\n\
# HELP synrepo_raw_file_tokens_total Estimated: sum of estimated raw-file tokens that served cards replaced.\n\
# TYPE synrepo_raw_file_tokens_total counter\n\
synrepo_raw_file_tokens_total 2400\n\
# HELP synrepo_estimated_tokens_saved_total Estimated: cold-file-read avoidance from raw-file vs card token comparisons. Not direct proof of avoided external reads.\n\
# TYPE synrepo_estimated_tokens_saved_total counter\n\
synrepo_estimated_tokens_saved_total 2160\n\
# HELP synrepo_stale_responses_total Observed: number of responses that surfaced stale advisory content.\n\
# TYPE synrepo_stale_responses_total counter\n\
synrepo_stale_responses_total 1\n\
# HELP synrepo_truncation_applied_total Observed: number of responses that applied token-budget truncation.\n\
# TYPE synrepo_truncation_applied_total counter\n\
synrepo_truncation_applied_total 0\n\
# HELP synrepo_test_surface_hits_total Observed: number of test-surface responses with at least one discovered test.\n\
# TYPE synrepo_test_surface_hits_total counter\n\
synrepo_test_surface_hits_total 2\n\
	# HELP synrepo_changed_files_total Observed: number of changed files observed by synrepo_changed.\n\
	# TYPE synrepo_changed_files_total counter\n\
	synrepo_changed_files_total 4\n\
	# HELP synrepo_mcp_requests_total Observed: repository-scoped MCP requests that reached a prepared runtime.\n\
	# TYPE synrepo_mcp_requests_total counter\n\
	synrepo_mcp_requests_total 4\n\
	# HELP synrepo_mcp_resource_reads_total Observed: MCP resource reads that reached a prepared repository.\n\
	# TYPE synrepo_mcp_resource_reads_total counter\n\
	synrepo_mcp_resource_reads_total 1\n\
	# HELP synrepo_compact_outputs_total Observed: compact MCP read outputs served.\n\
	# TYPE synrepo_compact_outputs_total counter\n\
	synrepo_compact_outputs_total 2\n\
	# HELP synrepo_compact_returned_tokens_total Estimated: sum of estimated tokens in compact outputs.\n\
	# TYPE synrepo_compact_returned_tokens_total counter\n\
	synrepo_compact_returned_tokens_total 80\n\
	# HELP synrepo_compact_original_tokens_total Estimated: sum of estimated tokens in uncompact output shapes.\n\
	# TYPE synrepo_compact_original_tokens_total counter\n\
	synrepo_compact_original_tokens_total 400\n\
	# HELP synrepo_compact_estimated_tokens_saved_total Estimated: token savings from compact output comparisons.\n\
	# TYPE synrepo_compact_estimated_tokens_saved_total counter\n\
	synrepo_compact_estimated_tokens_saved_total 320\n\
	# HELP synrepo_compact_omitted_items_total Observed: search rows or compactable items omitted from compact outputs.\n\
	# TYPE synrepo_compact_omitted_items_total counter\n\
	synrepo_compact_omitted_items_total 5\n\
	# HELP synrepo_compact_truncation_applied_total Observed: compact outputs that omitted content.\n\
	# TYPE synrepo_compact_truncation_applied_total counter\n\
	synrepo_compact_truncation_applied_total 1\n\
	# HELP synrepo_route_classifications_total Observed: task-route classifications served without storing task text.\n\
	# TYPE synrepo_route_classifications_total counter\n\
	synrepo_route_classifications_total 6\n\
	# HELP synrepo_context_fast_path_signals_total Observed: hook emissions containing the context fast-path signal.\n\
	# TYPE synrepo_context_fast_path_signals_total counter\n\
	synrepo_context_fast_path_signals_total 3\n\
	# HELP synrepo_deterministic_edit_candidates_total Observed: route classifications that returned a deterministic edit candidate.\n\
	# TYPE synrepo_deterministic_edit_candidates_total counter\n\
	synrepo_deterministic_edit_candidates_total 2\n\
	# HELP synrepo_deterministic_edit_candidate_signals_total Observed: hook emissions containing the deterministic edit candidate signal.\n\
	# TYPE synrepo_deterministic_edit_candidate_signals_total counter\n\
	synrepo_deterministic_edit_candidate_signals_total 1\n\
	# HELP synrepo_llm_not_required_signals_total Observed: hook emissions containing the LLM-not-required signal.\n\
	# TYPE synrepo_llm_not_required_signals_total counter\n\
	synrepo_llm_not_required_signals_total 3\n\
	# HELP synrepo_anchored_edit_accepted_total Observed: anchored edit operations accepted by the gated edit surface.\n\
	# TYPE synrepo_anchored_edit_accepted_total counter\n\
	synrepo_anchored_edit_accepted_total 2\n\
	# HELP synrepo_anchored_edit_rejected_total Observed: anchored edit operations rejected by the gated edit surface.\n\
	# TYPE synrepo_anchored_edit_rejected_total counter\n\
	synrepo_anchored_edit_rejected_total 1\n\
	# HELP synrepo_cross_link_generation_total Observed: cross-link candidate pairs sent to the configured generator.\n\
	# TYPE synrepo_cross_link_generation_total counter\n\
	synrepo_cross_link_generation_total 7\n\
	# HELP synrepo_cross_link_promoted_total Observed: proposed cross-links promoted into the graph.\n\
	# TYPE synrepo_cross_link_promoted_total counter\n\
	synrepo_cross_link_promoted_total 2\n\
	# HELP synrepo_commentary_refresh_total Observed: commentary refresh attempts.\n\
	# TYPE synrepo_commentary_refresh_total counter\n\
	synrepo_commentary_refresh_total 5\n\
	# HELP synrepo_commentary_refresh_errors_total Observed: commentary refresh attempts that returned an error.\n\
	# TYPE synrepo_commentary_refresh_errors_total counter\n\
	synrepo_commentary_refresh_errors_total 1\n\
	# HELP synrepo_estimated_llm_calls_avoided_total Estimated: route or hook recommendations where an LLM call was likely avoidable.\n\
	# TYPE synrepo_estimated_llm_calls_avoided_total counter\n\
	synrepo_estimated_llm_calls_avoided_total 5\n\
	# HELP synrepo_budget_tier_usage Observed: count of card responses by budget tier.\n\
	# TYPE synrepo_budget_tier_usage counter\n\
	synrepo_budget_tier_usage{tier=\"normal\"} 1\n\
	synrepo_budget_tier_usage{tier=\"tiny\"} 2\n\
	# HELP synrepo_mcp_tool_calls_total Observed: MCP tool calls keyed by tool name.\n\
	# TYPE synrepo_mcp_tool_calls_total counter\n\
	synrepo_mcp_tool_calls_total{tool=\"synrepo_search\"} 2\n\
	# HELP synrepo_mcp_tool_errors_total Observed: MCP tool responses with a top-level error field.\n\
	# TYPE synrepo_mcp_tool_errors_total counter\n\
	synrepo_mcp_tool_errors_total{tool=\"synrepo_search\"} 1\n\
	# HELP synrepo_saved_context_writes_total Observed: explicit advisory saved-context mutations keyed by operation.\n\
	# TYPE synrepo_saved_context_writes_total counter\n\
	synrepo_saved_context_writes_total{operation=\"note_add\"} 1\n\
	# HELP synrepo_workflow_calls_total Observed: workflow alias tool-call counts (orient, find, explain, impact, risks, tests, changed, minimum_context).\n\
	# TYPE synrepo_workflow_calls_total counter\n\
synrepo_workflow_calls_total{tool=\"find\"} 1\n\
synrepo_workflow_calls_total{tool=\"orient\"} 2\n";

    assert_eq!(metrics.to_prometheus_text(), expected);
}

#[test]
fn prometheus_output_is_empty_when_no_tiers() {
    let metrics = ContextMetrics::default();
    let text = metrics.to_prometheus_text();
    assert!(text.contains("synrepo_cards_served_total 0"));
    assert!(
        !text.contains("synrepo_budget_tier_usage{"),
        "budget tier block must emit no rows when the map is empty"
    );
    assert!(
        !text.contains("synrepo_workflow_calls_total{"),
        "workflow calls block must emit no rows when the map is empty"
    );
    assert!(
        !text.contains("synrepo_mcp_tool_calls_total{"),
        "MCP tool calls block must emit no rows when the map is empty"
    );
}
