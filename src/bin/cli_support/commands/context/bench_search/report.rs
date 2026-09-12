//! Report shape, summary aggregation, and rendering for `bench search`.

use serde::Serialize;

use super::super::bench_shared::BenchTarget;

#[derive(Debug, Serialize)]
pub(super) struct BenchSearchReport {
    pub(super) schema_version: u32,
    pub(super) summary: BenchSearchSummary,
    pub(super) tasks: Vec<BenchSearchTaskReport>,
}

#[derive(Debug, Serialize)]
pub(super) struct BenchSearchSummary {
    pub(super) total_tasks: usize,
    pub(super) lexical_hit_at_5: Option<f64>,
    pub(super) auto_hit_at_5: Option<f64>,
    pub(super) dense_first_hit_at_5: Option<f64>,
    pub(super) lexical_latency_ms: Option<u64>,
    pub(super) auto_latency_ms: Option<u64>,
    pub(super) dense_first_latency_ms: Option<u64>,
    pub(super) semantic_available_tasks: usize,
    pub(super) hybrid_improved_tasks: usize,
    pub(super) hybrid_matched_tasks: usize,
    pub(super) hybrid_regressed_tasks: usize,
    pub(super) dense_first_vs_rrf_wins: usize,
    pub(super) dense_first_vs_rrf_regressions: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct BenchSearchTaskReport {
    pub(super) name: String,
    pub(super) category: String,
    pub(super) query: String,
    pub(super) lexical: Option<BenchSearchRun>,
    pub(super) auto: Option<BenchSearchRun>,
    pub(super) dense_first: Option<BenchSearchRun>,
}

#[derive(Debug, Serialize)]
pub(super) struct BenchSearchRun {
    pub(super) target_hit: bool,
    pub(super) target_hits: Vec<BenchTarget>,
    pub(super) target_misses: Vec<BenchTarget>,
    pub(super) returned_targets: Vec<String>,
    pub(super) returned_symbols: Vec<String>,
    pub(super) latency_ms: u64,
    pub(super) engine: String,
    pub(super) semantic_available: bool,
    pub(super) semantic_row_count: usize,
}

pub(super) fn summarize(tasks: &[BenchSearchTaskReport]) -> BenchSearchSummary {
    let lexical_runs = tasks.iter().filter_map(|task| task.lexical.as_ref());
    let auto_runs = tasks.iter().filter_map(|task| task.auto.as_ref());
    let dense_first_runs = tasks.iter().filter_map(|task| task.dense_first.as_ref());
    let lexical_hit_count = lexical_runs.clone().filter(|run| run.target_hit).count();
    let auto_hit_count = auto_runs.clone().filter(|run| run.target_hit).count();
    let dense_first_hit_count = dense_first_runs
        .clone()
        .filter(|run| run.target_hit)
        .count();
    let semantic_available_tasks = auto_runs
        .clone()
        .filter(|run| run.semantic_available)
        .count();
    let mut improved = 0;
    let mut regressed = 0;
    // dense-first vs auto (RRF) — these are the arms whose comparison matters.
    let mut dense_wins = 0;
    let mut dense_regressions = 0;
    for task in tasks {
        if let (Some(lexical), Some(auto)) = (&task.lexical, &task.auto) {
            match (lexical.target_hit, auto.target_hit) {
                (false, true) => improved += 1,
                (true, false) => regressed += 1,
                _ => {}
            }
        }
        if let (Some(dense), Some(auto)) = (&task.dense_first, &task.auto) {
            match (auto.target_hit, dense.target_hit) {
                (false, true) => dense_wins += 1,
                (true, false) => dense_regressions += 1,
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
        dense_first_hit_at_5: ratio(
            dense_first_hit_count,
            tasks.iter().filter(|t| t.dense_first.is_some()).count(),
        ),
        lexical_latency_ms: latency_sum(tasks.iter().filter_map(|task| task.lexical.as_ref())),
        auto_latency_ms: latency_sum(tasks.iter().filter_map(|task| task.auto.as_ref())),
        dense_first_latency_ms: latency_sum(
            tasks.iter().filter_map(|task| task.dense_first.as_ref()),
        ),
        semantic_available_tasks,
        hybrid_improved_tasks: improved,
        hybrid_matched_tasks: tasks.len().saturating_sub(improved + regressed),
        hybrid_regressed_tasks: regressed,
        dense_first_vs_rrf_wins: dense_wins,
        dense_first_vs_rrf_regressions: dense_regressions,
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

pub(super) fn render_report(report: &BenchSearchReport, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "search benchmark (schema v{}): {} task(s)",
            report.schema_version, report.summary.total_tasks
        );
        println!(
            "  hit@5 lexical={:?} auto={:?} dense-first={:?}; semantic_available_tasks={}",
            report.summary.lexical_hit_at_5,
            report.summary.auto_hit_at_5,
            report.summary.dense_first_hit_at_5,
            report.summary.semantic_available_tasks
        );
        println!(
            "  hybrid (auto vs lexical): improved={} matched={} regressed={}",
            report.summary.hybrid_improved_tasks,
            report.summary.hybrid_matched_tasks,
            report.summary.hybrid_regressed_tasks
        );
        println!(
            "  dense-first vs auto (RRF): wins={} regressions={}",
            report.summary.dense_first_vs_rrf_wins, report.summary.dense_first_vs_rrf_regressions
        );
        println!(
            "  total latency: lexical={:?}ms auto={:?}ms dense-first={:?}ms",
            report.summary.lexical_latency_ms,
            report.summary.auto_latency_ms,
            report.summary.dense_first_latency_ms
        );
    }
    Ok(())
}
