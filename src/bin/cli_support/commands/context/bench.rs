//! `synrepo bench context` — fixture-backed context-savings benchmark.
//!
//! # Fixture schema (schema_version 1)
//!
//! Each fixture under `benches/tasks/*.json` describes one workflow query.
//! The benchmark reads fixtures only and never writes graph or overlay data.
//!
//! Required fields:
//!
//! - `query`: non-empty plain-language query text.
//! - `category`: workflow category. Known categories are `route_to_edit`,
//!   `symbol_explanation`, `impact_or_risk`, `test_surface`. Unknown
//!   categories are kept verbatim so teams can pilot new ones, but a fixture
//!   set used for release evidence SHOULD cover more than one known category
//!   (see `specs/evaluation/spec.md`).
//!
//! Optional fields:
//!
//! - `name`: display name for the task report. Defaults to the fixture path.
//! - `required_targets`: array of `{ kind, value }` entries the card path must
//!   surface. When non-empty, every target is checked; any missing entry is
//!   recorded as a miss. Valid `kind` values: `file`, `symbol`, `test`.
//!
//! Validation rejects:
//!
//! - Missing or whitespace-only `query`.
//! - `required_targets` entries with empty `value`.
//! - `required_targets` entries whose `kind` is not in the known set.
//!
//! # Report schema (schema_version 1)
//!
//! The JSON report is stable across patch releases unless `schema_version`
//! changes. Documentation numeric claims must cite a benchmark report,
//! including reduction ratio, target hit/miss, stale rate, and latency.

use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use synrepo::config::Config;
use synrepo::surface::card::{Budget, CardCompiler};

use super::super::mcp_runtime::prepare_state;
use super::bench_shared::{
    classify_targets, expand_task_glob, validate_fixture, BenchTarget, BenchTask, KNOWN_CATEGORIES,
};

/// Current benchmark report schema version.
///
/// Bump this only when a field is renamed or removed in a way that breaks
/// consumers. Additive field changes keep the same version.
const SCHEMA_VERSION: u32 = 1;

/// Baseline kind reported in each task output.
///
/// Reflects the comparison point for `raw_file_tokens`: the estimated tokens
/// an agent would have spent reading the raw source files that back the
/// returned cards. Defined as a stable enum-like string so future baselines
/// (for example `full_repo_read`) can coexist.
const BASELINE_KIND_RAW_FILE: &str = "raw_file";

/// Upper bound on search matches examined per fixture.
const MAX_SEARCH_MATCHES: usize = 10;

/// Upper bound on card responses kept per fixture. Must be <= MAX_SEARCH_MATCHES
/// since we iterate matches and stop once this many distinct cards resolve.
const MAX_RETURNED_CARDS: usize = 5;

pub(crate) fn bench_context(
    repo_root: &Path,
    tasks_glob: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let state = prepare_state(repo_root)?;
    let compiler = state
        .create_read_compiler()
        .map_err(|error| anyhow::anyhow!(error))?;
    let task_paths = expand_task_glob(repo_root, tasks_glob)?;
    let mut tasks = Vec::new();

    for path in task_paths {
        let fixture: BenchTask = serde_json::from_slice(&std::fs::read(&path)?)
            .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?;
        validate_fixture(&fixture)
            .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?;

        let start = Instant::now();
        let matches = synrepo::substrate::search(&config, repo_root, &fixture.query)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut card_tokens = 0usize;
        let mut raw_tokens = 0usize;
        let mut returned_paths = Vec::new();
        let mut returned_symbols: Vec<String> = Vec::new();
        let mut cards_examined = 0usize;
        let mut stale_cards = 0usize;

        for m in matches.iter().take(MAX_SEARCH_MATCHES) {
            let rel = m.path.to_string_lossy().to_string();
            if !seen.insert(rel.clone()) {
                continue;
            }
            if let Some(file) = compiler.reader().file_by_path(&rel)? {
                let card = compiler.file_card(file.id, Budget::Tiny)?;
                cards_examined += 1;
                if card.context_accounting.stale {
                    stale_cards += 1;
                }
                card_tokens += card.context_accounting.token_estimate;
                raw_tokens += card.context_accounting.raw_file_token_estimate;
                returned_paths.push(card.path);
                for sym in &card.symbols {
                    returned_symbols.push(sym.qualified_name.clone());
                }
            }
            if returned_paths.len() >= MAX_RETURNED_CARDS {
                break;
            }
        }

        let (target_hits, target_misses) = classify_targets(
            &fixture.required_targets,
            &returned_paths,
            &returned_symbols,
        );
        let target_hit = target_misses.is_empty();
        let reduction_ratio = if raw_tokens > 0 {
            raw_tokens.saturating_sub(card_tokens) as f64 / raw_tokens as f64
        } else {
            0.0
        };
        let stale_rate = if cards_examined > 0 {
            stale_cards as f64 / cards_examined as f64
        } else {
            0.0
        };

        tasks.push(BenchTaskReport {
            name: fixture.name.unwrap_or_else(|| path.display().to_string()),
            category: fixture.category,
            query: fixture.query,
            baseline_kind: BASELINE_KIND_RAW_FILE.to_string(),
            raw_file_tokens: raw_tokens,
            card_tokens,
            reduction_ratio,
            target_hit,
            target_hits,
            target_misses,
            stale_rate,
            latency_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            returned_targets: returned_paths,
        });
    }

    let summary = summarize(&tasks);
    let report = BenchContextReport {
        schema_version: SCHEMA_VERSION,
        summary,
        tasks,
    };
    render_report(&report, json_output)
}

fn render_report(report: &BenchContextReport, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "context benchmark (schema v{}): {} task(s)",
            report.schema_version,
            report.tasks.len()
        );
        for task in &report.tasks {
            println!(
                "  [{}] {}: {:.1}% reduction, hit={}, misses={}, stale_rate={:.2}",
                task.category,
                task.name,
                task.reduction_ratio * 100.0,
                task.target_hit,
                task.target_misses.len(),
                task.stale_rate,
            );
        }
        if !report.summary.missing_categories.is_empty() {
            println!(
                "  missing categories (known set not covered): {}",
                report.summary.missing_categories.join(", ")
            );
        }
    }
    Ok(())
}

fn summarize(tasks: &[BenchTaskReport]) -> BenchContextSummary {
    let total_tasks = tasks.len();
    let tasks_with_hits = tasks.iter().filter(|t| !t.target_hits.is_empty()).count();
    let tasks_with_misses = tasks.iter().filter(|t| !t.target_misses.is_empty()).count();
    let mut categories: Vec<String> = tasks.iter().map(|t| t.category.clone()).collect();
    categories.sort();
    categories.dedup();
    let missing_categories: Vec<String> = KNOWN_CATEGORIES
        .iter()
        .filter(|known| !categories.iter().any(|c| c == *known))
        .map(|k| (*k).to_string())
        .collect();
    BenchContextSummary {
        total_tasks,
        tasks_with_hits,
        tasks_with_misses,
        categories,
        missing_categories,
    }
}

#[derive(Debug, Serialize)]
struct BenchContextReport {
    schema_version: u32,
    summary: BenchContextSummary,
    tasks: Vec<BenchTaskReport>,
}

#[derive(Debug, Serialize)]
struct BenchContextSummary {
    total_tasks: usize,
    tasks_with_hits: usize,
    tasks_with_misses: usize,
    categories: Vec<String>,
    missing_categories: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchTaskReport {
    name: String,
    category: String,
    query: String,
    baseline_kind: String,
    raw_file_tokens: usize,
    card_tokens: usize,
    /// `(raw - card) / raw`; zero when `raw_file_tokens` is zero.
    reduction_ratio: f64,
    /// True iff `target_misses.is_empty()`. Kept as its own field because the
    /// stable JSON schema locks it; a renderer may cite `target_hit` without
    /// walking the misses vec.
    target_hit: bool,
    target_hits: Vec<BenchTarget>,
    target_misses: Vec<BenchTarget>,
    stale_rate: f64,
    latency_ms: u64,
    returned_targets: Vec<String>,
}

#[cfg(test)]
mod tests;
