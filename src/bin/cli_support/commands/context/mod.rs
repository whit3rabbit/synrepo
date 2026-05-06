//! `synrepo` context-serving command surface: dashboard aliases and stats.

mod bench;
mod bench_search;
mod bench_shared;

use std::path::Path;

use synrepo::config::Config;
use synrepo::surface::card::Budget;
use synrepo::surface::mcp::{cards, search};

use super::mcp_runtime::prepare_state;
use crate::cli_support::cli_args::StatFormatArg;

pub(crate) use bench::bench_context;
pub(crate) use bench_search::bench_search;

pub(crate) fn cards_alias(
    repo_root: &Path,
    query: &str,
    budget_tokens: Option<usize>,
) -> anyhow::Result<()> {
    let state = prepare_state(repo_root)?;
    print!(
        "{}",
        search::handle_where_to_edit(&state, query.to_string(), 5, budget_tokens)
    );
    Ok(())
}

pub(crate) fn explain_alias(
    repo_root: &Path,
    target: &str,
    budget_tokens: Option<usize>,
) -> anyhow::Result<()> {
    let state = prepare_state(repo_root)?;
    let budget = tier_for_budget_tokens(budget_tokens);
    print!(
        "{}",
        cards::handle_card(&state, target.to_string(), budget, budget_tokens, false)
    );
    Ok(())
}

pub(crate) fn impact_alias(
    repo_root: &Path,
    target: &str,
    budget_tokens: Option<usize>,
) -> anyhow::Result<()> {
    let state = prepare_state(repo_root)?;
    let budget = tier_for_budget_tokens(budget_tokens);
    print!(
        "{}",
        cards::handle_change_risk(&state, target.to_string(), budget, budget_tokens)
    );
    Ok(())
}

pub(crate) fn tests_alias(
    repo_root: &Path,
    target: &str,
    budget_tokens: Option<usize>,
) -> anyhow::Result<()> {
    let state = prepare_state(repo_root)?;
    let budget = tier_for_budget_tokens(budget_tokens);
    print!(
        "{}",
        cards::handle_test_surface(&state, target.to_string(), budget, budget_tokens)
    );
    Ok(())
}

pub(crate) fn risks_alias(
    repo_root: &Path,
    target: &str,
    budget_tokens: Option<usize>,
) -> anyhow::Result<()> {
    impact_alias(repo_root, target, budget_tokens)
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum StatFormat {
    #[default]
    Text,
    Json,
    Prometheus,
}

impl From<StatFormatArg> for StatFormat {
    fn from(arg: StatFormatArg) -> Self {
        match arg {
            StatFormatArg::Text => StatFormat::Text,
            StatFormatArg::Json => StatFormat::Json,
            StatFormatArg::Prometheus => StatFormat::Prometheus,
        }
    }
}

impl StatFormat {
    /// Resolve the effective format from the two CLI shapes: the new `--format`
    /// flag and the legacy `--json` boolean. `--format` wins when set; `--json`
    /// is the back-compat alias for `--format json`.
    pub(crate) fn from_cli(format: Option<StatFormatArg>, json: bool) -> Self {
        match (format, json) {
            (Some(arg), _) => arg.into(),
            (None, true) => StatFormat::Json,
            (None, false) => StatFormat::Text,
        }
    }
}

pub(crate) fn stats_context(repo_root: &Path, format: StatFormat) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let metrics = synrepo::pipeline::context_metrics::load(&synrepo_dir)?;
    match format {
        StatFormat::Json => println!("{}", serde_json::to_string_pretty(&metrics)?),
        StatFormat::Prometheus => print!("{}", metrics.to_prometheus_text()),
        StatFormat::Text => {
            println!("synrepo context stats");
            println!("  observed:");
            println!("    cards served: {}", metrics.cards_served_total);
            println!("    changed files: {}", metrics.changed_files_total);
            println!("    stale responses: {}", metrics.stale_responses_total);
            println!(
                "    truncation applied: {}",
                metrics.truncation_applied_total
            );
            println!(
                "    responses over soft cap: {}",
                metrics.responses_over_soft_cap_total
            );
            println!(
                "    responses truncated: {}",
                metrics.responses_truncated_total
            );
            println!("    deep cards served: {}", metrics.deep_cards_served_total);
            println!("    test surface hits: {}", metrics.test_surface_hits_total);
            if metrics.workflow_calls_total.is_empty() {
                println!("    workflow calls: (none recorded)");
            } else {
                println!("    workflow calls:");
                for (tool, count) in &metrics.workflow_calls_total {
                    println!("      {tool}: {count}");
                }
            }
            println!("  estimated (from card accounting):");
            println!("    avg tokens/card: {:.1}", metrics.card_tokens_avg());
            println!(
                "    cold-file tokens avoided: {}",
                metrics.estimated_tokens_saved_total
            );
            println!("  compact output:");
            println!("    outputs served: {}", metrics.compact_outputs_total);
            println!(
                "    compact tokens avoided: {}",
                metrics.compact_estimated_tokens_saved_total
            );
            println!("    omitted items: {}", metrics.compact_omitted_items_total);
            println!("  context packs:");
            println!("    tokens served: {}", metrics.context_pack_tokens_total);
            println!("  response budget:");
            println!(
                "    largest response tokens: {}",
                metrics.largest_response_tokens
            );
        }
    }
    Ok(())
}

fn tier_for_budget_tokens(budget_tokens: Option<usize>) -> String {
    match budget_tokens {
        Some(tokens) if tokens <= Budget::Tiny.total_budget_tokens() => "tiny",
        Some(tokens) if tokens <= Budget::Normal.total_budget_tokens() => "normal",
        Some(_) => "deep",
        None => "tiny",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_for_budget_tokens_maps_caps_to_existing_tiers() {
        assert_eq!(tier_for_budget_tokens(Some(500)), "tiny");
        assert_eq!(tier_for_budget_tokens(Some(2_000)), "normal");
        assert_eq!(tier_for_budget_tokens(Some(9_000)), "deep");
        assert_eq!(tier_for_budget_tokens(None), "tiny");
    }
}
