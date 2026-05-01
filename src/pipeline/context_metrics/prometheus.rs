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
        write_labeled_counter(
            &mut out,
            "synrepo_saved_context_writes_total",
            "Observed: explicit advisory saved-context mutations keyed by operation.",
            "operation",
            &self.saved_context_writes_total,
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

fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
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
}
