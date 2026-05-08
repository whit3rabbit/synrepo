//! Prometheus text-exposition rendering for [`ContextMetrics`].
//!
//! Help text on every counter labels the value as **Observed** (direct call
//! or response count) or **Estimated** (derived from card-accounting token
//! comparisons). Scrapers MUST preserve that distinction.

use super::ContextMetrics;

impl ContextMetrics {
    /// Emit these metrics in Prometheus text exposition format (version 0.0.4).
    ///
    /// Names are prefixed `synrepo_` and counters carry the `_total` suffix
    /// where appropriate. Help text labels each line as **observed** or
    /// **estimated** so scrapers can separate direct call counts from
    /// card-accounting estimates. The metric set is intentionally stable:
    /// adding or renaming a line is a breaking change for scrapers.
    pub fn to_prometheus_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();

        write_counter(
            &mut out,
            "synrepo_cards_served_total",
            "Observed: total number of card-shaped responses served.",
            self.cards_served_total,
        );
        write_counter(
            &mut out,
            "synrepo_card_tokens_total",
            "Estimated: sum of estimated tokens in served cards.",
            self.card_tokens_total,
        );
        write_counter(
            &mut out,
            "synrepo_raw_file_tokens_total",
            "Estimated: sum of estimated raw-file tokens that served cards replaced.",
            self.raw_file_tokens_total,
        );
        write_counter(
            &mut out,
            "synrepo_estimated_tokens_saved_total",
            "Estimated: cold-file-read avoidance from raw-file vs card token comparisons. Not direct proof of avoided external reads.",
            self.estimated_tokens_saved_total,
        );
        write_counter(
            &mut out,
            "synrepo_stale_responses_total",
            "Observed: number of responses that surfaced stale advisory content.",
            self.stale_responses_total,
        );
        write_counter(
            &mut out,
            "synrepo_truncation_applied_total",
            "Observed: number of responses that applied token-budget truncation.",
            self.truncation_applied_total,
        );
        write_counter(
            &mut out,
            "synrepo_test_surface_hits_total",
            "Observed: number of test-surface responses with at least one discovered test.",
            self.test_surface_hits_total,
        );
        write_counter(
            &mut out,
            "synrepo_changed_files_total",
            "Observed: number of changed files observed by synrepo_changed.",
            self.changed_files_total,
        );
        write_counter(
            &mut out,
            "synrepo_mcp_requests_total",
            "Observed: repository-scoped MCP requests that reached a prepared runtime.",
            self.mcp_requests_total,
        );
        write_counter(
            &mut out,
            "synrepo_mcp_resource_reads_total",
            "Observed: MCP resource reads that reached a prepared repository.",
            self.mcp_resource_reads_total,
        );
        write_counter(
            &mut out,
            "synrepo_compact_outputs_total",
            "Observed: compact MCP read outputs served.",
            self.compact_outputs_total,
        );
        write_counter(
            &mut out,
            "synrepo_compact_returned_tokens_total",
            "Estimated: sum of estimated tokens in compact outputs.",
            self.compact_returned_tokens_total,
        );
        write_counter(
            &mut out,
            "synrepo_compact_original_tokens_total",
            "Estimated: sum of estimated tokens in uncompact output shapes.",
            self.compact_original_tokens_total,
        );
        write_counter(
            &mut out,
            "synrepo_compact_estimated_tokens_saved_total",
            "Estimated: token savings from compact output comparisons.",
            self.compact_estimated_tokens_saved_total,
        );
        write_counter(
            &mut out,
            "synrepo_compact_omitted_items_total",
            "Observed: search rows or compactable items omitted from compact outputs.",
            self.compact_omitted_items_total,
        );
        write_counter(
            &mut out,
            "synrepo_compact_truncation_applied_total",
            "Observed: compact outputs that omitted content.",
            self.compact_truncation_applied_total,
        );
        write_counter(
            &mut out,
            "synrepo_responses_over_soft_cap_total",
            "Observed: MCP responses whose estimated size exceeded the soft cap.",
            self.responses_over_soft_cap_total,
        );
        write_counter(
            &mut out,
            "synrepo_responses_truncated_total",
            "Observed: MCP responses trimmed by the final response clamp.",
            self.responses_truncated_total,
        );
        write_counter(
            &mut out,
            "synrepo_deep_cards_served_total",
            "Observed: deep card-shaped responses served.",
            self.deep_cards_served_total,
        );
        write_counter(
            &mut out,
            "synrepo_context_pack_tokens_total",
            "Estimated: sum of estimated tokens in served context packs.",
            self.context_pack_tokens_total,
        );
        write_counter(
            &mut out,
            "synrepo_resume_context_responses_total",
            "Observed: repo resume context packets served without storing packet content.",
            self.resume_context_responses_total,
        );
        write_counter(
            &mut out,
            "synrepo_resume_context_tokens_total",
            "Estimated: sum of estimated tokens in served repo resume packets.",
            self.resume_context_tokens_total,
        );
        write_counter(
            &mut out,
            "synrepo_largest_response_tokens",
            "Estimated: largest MCP response token estimate observed.",
            self.largest_response_tokens,
        );
        write_counter(
            &mut out,
            "synrepo_route_classifications_total",
            "Observed: task-route classifications served without storing task text.",
            self.route_classifications_total,
        );
        write_counter(
            &mut out,
            "synrepo_context_fast_path_signals_total",
            "Observed: hook emissions containing the context fast-path signal.",
            self.context_fast_path_signals_total,
        );
        write_counter(
            &mut out,
            "synrepo_deterministic_edit_candidates_total",
            "Observed: route classifications that returned a deterministic edit candidate.",
            self.deterministic_edit_candidates_total,
        );
        write_counter(
            &mut out,
            "synrepo_deterministic_edit_candidate_signals_total",
            "Observed: hook emissions containing the deterministic edit candidate signal.",
            self.deterministic_edit_candidate_signals_total,
        );
        write_counter(
            &mut out,
            "synrepo_llm_not_required_signals_total",
            "Observed: hook emissions containing the LLM-not-required signal.",
            self.llm_not_required_signals_total,
        );
        write_counter(
            &mut out,
            "synrepo_anchored_edit_accepted_total",
            "Observed: anchored edit operations accepted by the gated edit surface.",
            self.anchored_edit_accepted_total,
        );
        write_counter(
            &mut out,
            "synrepo_anchored_edit_rejected_total",
            "Observed: anchored edit operations rejected by the gated edit surface.",
            self.anchored_edit_rejected_total,
        );
        write_counter(
            &mut out,
            "synrepo_cross_link_generation_total",
            "Observed: cross-link candidate pairs sent to the configured generator.",
            self.cross_link_generation_total,
        );
        write_counter(
            &mut out,
            "synrepo_cross_link_promoted_total",
            "Observed: proposed cross-links promoted into the graph.",
            self.cross_link_promoted_total,
        );
        write_counter(
            &mut out,
            "synrepo_commentary_refresh_total",
            "Observed: commentary refresh attempts.",
            self.commentary_refresh_total,
        );
        write_counter(
            &mut out,
            "synrepo_commentary_refresh_errors_total",
            "Observed: commentary refresh attempts that returned an error.",
            self.commentary_refresh_errors_total,
        );
        write_counter(
            &mut out,
            "synrepo_estimated_llm_calls_avoided_total",
            "Estimated: route or hook recommendations where an LLM call was likely avoidable.",
            self.estimated_llm_calls_avoided_total,
        );

        writeln!(
            out,
            "# HELP synrepo_budget_tier_usage Observed: count of card responses by budget tier."
        )
        .unwrap();
        writeln!(out, "# TYPE synrepo_budget_tier_usage counter").unwrap();
        for (tier, count) in &self.budget_tier_usage {
            writeln!(
                out,
                "synrepo_budget_tier_usage{{tier=\"{}\"}} {}",
                escape_label_value(tier),
                count
            )
            .unwrap();
        }

        write_labeled_counter(
            &mut out,
            "synrepo_mcp_tool_calls_total",
            "Observed: MCP tool calls keyed by tool name.",
            "tool",
            &self.mcp_tool_calls_total,
        );
        write_labeled_counter(
            &mut out,
            "synrepo_mcp_tool_errors_total",
            "Observed: MCP tool responses with a top-level error field.",
            "tool",
            &self.mcp_tool_errors_total,
        );
        write_tool_error_code_counter(&mut out, &self.mcp_tool_error_codes_total);
        write_labeled_counter(
            &mut out,
            "synrepo_saved_context_writes_total",
            "Observed: explicit advisory saved-context mutations keyed by operation.",
            "operation",
            &self.saved_context_writes_total,
        );
        write_labeled_counter(
            &mut out,
            "synrepo_tool_token_totals",
            "Estimated: MCP response token estimates keyed by tool name.",
            "tool",
            &self.tool_token_totals,
        );

        writeln!(
            out,
            "# HELP synrepo_workflow_calls_total Observed: workflow alias tool-call counts (orient, find, explain, impact, risks, tests, changed, minimum_context)."
        )
        .unwrap();
        writeln!(out, "# TYPE synrepo_workflow_calls_total counter").unwrap();
        for (tool, count) in &self.workflow_calls_total {
            writeln!(
                out,
                "synrepo_workflow_calls_total{{tool=\"{}\"}} {}",
                escape_label_value(tool),
                count
            )
            .unwrap();
        }

        out
    }
}

fn write_counter(out: &mut String, name: &str, help: &str, value: u64) {
    use std::fmt::Write as _;
    writeln!(out, "# HELP {name} {help}").unwrap();
    writeln!(out, "# TYPE {name} counter").unwrap();
    writeln!(out, "{name} {value}").unwrap();
}

fn write_labeled_counter(
    out: &mut String,
    name: &str,
    help: &str,
    label: &str,
    values: &std::collections::BTreeMap<String, u64>,
) {
    use std::fmt::Write as _;
    writeln!(out, "# HELP {name} {help}").unwrap();
    writeln!(out, "# TYPE {name} counter").unwrap();
    for (key, count) in values {
        writeln!(
            out,
            "{name}{{{label}=\"{}\"}} {}",
            escape_label_value(key),
            count
        )
        .unwrap();
    }
}

fn write_tool_error_code_counter(
    out: &mut String,
    values: &std::collections::BTreeMap<String, std::collections::BTreeMap<String, u64>>,
) {
    use std::fmt::Write as _;
    writeln!(
        out,
        "# HELP synrepo_mcp_tool_error_codes_total Observed: MCP tool errors keyed by tool and stable error code."
    )
    .unwrap();
    writeln!(out, "# TYPE synrepo_mcp_tool_error_codes_total counter").unwrap();
    for (tool, codes) in values {
        for (code, count) in codes {
            writeln!(
                out,
                "synrepo_mcp_tool_error_codes_total{{tool=\"{}\",code=\"{}\"}} {}",
                escape_label_value(tool),
                escape_label_value(code),
                count
            )
            .unwrap();
        }
    }
}

fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests;
