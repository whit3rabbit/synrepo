//! `synrepo bench search` — fixture-backed lexical vs hybrid search eval.

use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use synrepo::core::ids::SymbolNodeId;
use synrepo::substrate::{HybridSearchRow, HybridSearchSource};
use synrepo::surface::card::compiler::GraphCardCompiler;
use syntext::SearchOptions;

use super::super::mcp_runtime::prepare_state;
use super::bench_shared::{
    classify_targets, expand_task_glob, validate_fixture, BenchTarget, BenchTask,
};

const SCHEMA_VERSION: u32 = 1;
const HIT_LIMIT: usize = 5;

pub(crate) fn bench_search(
    repo_root: &Path,
    tasks_glob: &str,
    mode: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let mode = BenchSearchMode::parse(mode)?;
    let config = synrepo::config::Config::load(repo_root)?;
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

        let lexical = if mode.includes_lexical() {
            Some(run_lexical(repo_root, &config, &fixture)?)
        } else {
            None
        };
        let auto = if mode.includes_auto() {
            Some(run_auto(repo_root, &config, &compiler, &fixture)?)
        } else {
            None
        };
        tasks.push(BenchSearchTaskReport {
            name: fixture.name.unwrap_or_else(|| path.display().to_string()),
            category: fixture.category,
            query: fixture.query,
            lexical,
            auto,
        });
    }

    let report = BenchSearchReport {
        schema_version: SCHEMA_VERSION,
        summary: summarize(&tasks),
        tasks,
    };
    render_report(&report, json_output)
}

fn run_lexical(
    repo_root: &Path,
    config: &synrepo::config::Config,
    fixture: &BenchTask,
) -> anyhow::Result<BenchSearchRun> {
    let start = Instant::now();
    let matches = synrepo::substrate::search_with_options(
        config,
        repo_root,
        &fixture.query,
        &search_options(),
    )?;
    let returned_targets = matches
        .into_iter()
        .map(|m| m.path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    Ok(build_run(
        "syntext",
        false,
        0,
        start,
        &fixture.required_targets,
        returned_targets,
        Vec::new(),
    ))
}

fn run_auto(
    repo_root: &Path,
    config: &synrepo::config::Config,
    compiler: &GraphCardCompiler,
    fixture: &BenchTask,
) -> anyhow::Result<BenchSearchRun> {
    let start = Instant::now();
    let report =
        synrepo::substrate::hybrid_search(config, repo_root, &fixture.query, &search_options())?;
    let semantic_row_count = report
        .rows
        .iter()
        .filter(|row| row.source != HybridSearchSource::Lexical)
        .count();
    let (returned_targets, returned_symbols) = returned_from_hybrid_rows(compiler, report.rows);
    Ok(build_run(
        report.engine,
        report.semantic_available,
        semantic_row_count,
        start,
        &fixture.required_targets,
        returned_targets,
        returned_symbols,
    ))
}

fn build_run(
    engine: &str,
    semantic_available: bool,
    semantic_row_count: usize,
    start: Instant,
    required_targets: &[BenchTarget],
    returned_targets: Vec<String>,
    returned_symbols: Vec<String>,
) -> BenchSearchRun {
    let (target_hits, target_misses) =
        classify_targets(required_targets, &returned_targets, &returned_symbols);
    BenchSearchRun {
        target_hit: target_misses.is_empty(),
        target_hits,
        target_misses,
        returned_targets,
        returned_symbols,
        latency_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        engine: engine.to_string(),
        semantic_available,
        semantic_row_count,
    }
}

fn returned_from_hybrid_rows(
    compiler: &GraphCardCompiler,
    rows: Vec<HybridSearchRow>,
) -> (Vec<String>, Vec<String>) {
    let mut targets = Vec::new();
    let mut symbols = Vec::new();
    for row in rows {
        if let Some(path) = row.path {
            targets.push(path);
        }
        if let Some(symbol_id) = row.symbol_id {
            if let Some((path, qname)) = symbol_details(compiler, symbol_id) {
                targets.push(path);
                symbols.push(qname);
            }
        }
    }
    targets.sort();
    targets.dedup();
    symbols.sort();
    symbols.dedup();
    (targets, symbols)
}

fn symbol_details(compiler: &GraphCardCompiler, id: SymbolNodeId) -> Option<(String, String)> {
    let symbol = compiler.reader().get_symbol(id).ok().flatten()?;
    let file = compiler.reader().get_file(symbol.file_id).ok().flatten()?;
    Some((file.path, symbol.qualified_name))
}

fn search_options() -> SearchOptions {
    SearchOptions {
        path_filter: None,
        file_type: None,
        exclude_type: None,
        max_results: Some(HIT_LIMIT),
        case_insensitive: false,
    }
}

fn summarize(tasks: &[BenchSearchTaskReport]) -> BenchSearchSummary {
    let lexical_runs = tasks.iter().filter_map(|task| task.lexical.as_ref());
    let auto_runs = tasks.iter().filter_map(|task| task.auto.as_ref());
    let lexical_hit_count = lexical_runs.clone().filter(|run| run.target_hit).count();
    let auto_hit_count = auto_runs.clone().filter(|run| run.target_hit).count();
    let semantic_available_tasks = auto_runs
        .clone()
        .filter(|run| run.semantic_available)
        .count();
    let mut improved = 0;
    let mut regressed = 0;
    for task in tasks {
        if let (Some(lexical), Some(auto)) = (&task.lexical, &task.auto) {
            match (lexical.target_hit, auto.target_hit) {
                (false, true) => improved += 1,
                (true, false) => regressed += 1,
                _ => {}
            }
        }
    }
    BenchSearchSummary {
        total_tasks: tasks.len(),
        lexical_hit_at_5: ratio(
            lexical_hit_count,
            tasks.iter().filter(|t| t.lexical.is_some()).count(),
        ),
        auto_hit_at_5: ratio(
            auto_hit_count,
            tasks.iter().filter(|t| t.auto.is_some()).count(),
        ),
        lexical_latency_ms: latency_sum(tasks.iter().filter_map(|task| task.lexical.as_ref())),
        auto_latency_ms: latency_sum(tasks.iter().filter_map(|task| task.auto.as_ref())),
        semantic_available_tasks,
        hybrid_improved_tasks: improved,
        hybrid_matched_tasks: tasks.len().saturating_sub(improved + regressed),
        hybrid_regressed_tasks: regressed,
    }
}

fn ratio(count: usize, total: usize) -> Option<f64> {
    (total > 0).then_some(count as f64 / total as f64)
}

fn latency_sum<'a>(runs: impl Iterator<Item = &'a BenchSearchRun>) -> Option<u64> {
    let mut total = 0u64;
    let mut count = 0usize;
    for run in runs {
        total = total.saturating_add(run.latency_ms);
        count += 1;
    }
    (count > 0).then_some(total)
}

fn render_report(report: &BenchSearchReport, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "search benchmark (schema v{}): {} task(s)",
            report.schema_version, report.summary.total_tasks
        );
        println!(
            "  hit@5 lexical={:?} auto={:?}; semantic_available_tasks={}",
            report.summary.lexical_hit_at_5,
            report.summary.auto_hit_at_5,
            report.summary.semantic_available_tasks
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BenchSearchMode {
    Lexical,
    Auto,
    Both,
}

impl BenchSearchMode {
    fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "lexical" => Ok(Self::Lexical),
            "auto" => Ok(Self::Auto),
            "both" => Ok(Self::Both),
            other => anyhow::bail!(
                "unknown bench search mode `{other}`; expected lexical, auto, or both"
            ),
        }
    }

    fn includes_lexical(self) -> bool {
        matches!(self, Self::Lexical | Self::Both)
    }

    fn includes_auto(self) -> bool {
        matches!(self, Self::Auto | Self::Both)
    }
}

#[derive(Debug, Serialize)]
struct BenchSearchReport {
    schema_version: u32,
    summary: BenchSearchSummary,
    tasks: Vec<BenchSearchTaskReport>,
}

#[derive(Debug, Serialize)]
struct BenchSearchSummary {
    total_tasks: usize,
    lexical_hit_at_5: Option<f64>,
    auto_hit_at_5: Option<f64>,
    lexical_latency_ms: Option<u64>,
    auto_latency_ms: Option<u64>,
    semantic_available_tasks: usize,
    hybrid_improved_tasks: usize,
    hybrid_matched_tasks: usize,
    hybrid_regressed_tasks: usize,
}

#[derive(Debug, Serialize)]
struct BenchSearchTaskReport {
    name: String,
    category: String,
    query: String,
    lexical: Option<BenchSearchRun>,
    auto: Option<BenchSearchRun>,
}

#[derive(Debug, Serialize)]
struct BenchSearchRun {
    target_hit: bool,
    target_hits: Vec<BenchTarget>,
    target_misses: Vec<BenchTarget>,
    returned_targets: Vec<String>,
    returned_symbols: Vec<String>,
    latency_ms: u64,
    engine: String,
    semantic_available: bool,
    semantic_row_count: usize,
}

#[cfg(test)]
mod tests;
