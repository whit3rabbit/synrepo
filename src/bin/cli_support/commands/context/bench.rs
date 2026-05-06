//! `synrepo bench context` — fixture-backed context-quality benchmark.
//!
//! # Fixture schema
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
//! - `scope`, `shape`, `ground`, and `budget`: optional `synrepo_ask` request
//!   controls used by the ask strategy.
//! - `expected_recipe`: optional task-context recipe expected from `synrepo_ask`.
//! - `allowed_context`: optional target allow-list used to compute
//!   `wrong_context_rate`.
//!
//! Validation rejects:
//!
//! - Missing or whitespace-only `query`.
//! - `required_targets` entries with empty `value`.
//! - `required_targets` entries whose `kind` is not in the known set.
//!
//! # Report schema (schema_version 2)
//!
//! The JSON report is stable across patch releases unless `schema_version`
//! changes. Documentation numeric claims must cite a benchmark report with
//! context-quality dimensions, not token reduction alone.

use std::path::Path;

use synrepo::config::Config;

use super::super::mcp_runtime::prepare_state;
#[cfg(test)]
use super::bench_shared::{classify_targets, wrong_context_rate, BenchTarget, KNOWN_CATEGORIES};
use super::bench_shared::{expand_task_glob, validate_fixture, BenchTask};
use mode::BenchContextMode;
use report::{summarize, BenchContextReport, BenchRunSet, BenchTaskReport, SCHEMA_VERSION};
#[cfg(test)]
use report::{BenchContextSummary, BASELINE_KIND_RAW_FILE};

mod mode;
mod report;
mod strategy;

pub(crate) fn bench_context(
    repo_root: &Path,
    tasks_glob: &str,
    mode: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let mode = BenchContextMode::parse(mode)?;
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

        let runs = run_strategies(repo_root, &config, &state, &compiler, &fixture, mode)?;
        tasks.push(BenchTaskReport::from_fixture(&path, &fixture, runs)?);
    }

    let report = BenchContextReport {
        schema_version: SCHEMA_VERSION,
        summary: summarize(&tasks),
        tasks,
    };
    render_report(&report, json_output)
}

fn run_strategies(
    repo_root: &Path,
    config: &Config,
    state: &synrepo::surface::mcp::SynrepoState,
    compiler: &synrepo::surface::card::compiler::GraphCardCompiler,
    fixture: &BenchTask,
    mode: BenchContextMode,
) -> anyhow::Result<BenchRunSet> {
    let mut runs = BenchRunSet::default();
    if mode.includes_raw_file() {
        runs.raw_file = Some(strategy::run_raw_file(
            repo_root, config, compiler, fixture,
        )?);
    }
    if mode.includes_lexical() {
        runs.lexical = Some(strategy::run_lexical(repo_root, config, fixture)?);
    }
    if mode.includes_cards() {
        runs.cards = Some(strategy::run_cards(repo_root, config, compiler, fixture)?);
    }
    if mode.includes_ask() {
        runs.ask = Some(strategy::run_ask(repo_root, state, fixture)?);
    }
    Ok(runs)
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
                "  [{}] {}: cards {:.1}% reduction, success={}, misses={}, stale_rate={:.2}",
                task.category,
                task.name,
                task.reduction_ratio * 100.0,
                task.runs.cards.as_ref().is_some_and(|run| run.task_success),
                task.target_misses.len(),
                task.stale_rate,
            );
            if let Some(ask) = &task.runs.ask {
                println!(
                    "      ask: success={}, citations={:.2}, spans={:.2}, wrong_context_rate={:?}",
                    ask.task_success,
                    ask.citation_coverage,
                    ask.span_coverage,
                    ask.wrong_context_rate,
                );
            }
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

#[cfg(test)]
mod tests;
