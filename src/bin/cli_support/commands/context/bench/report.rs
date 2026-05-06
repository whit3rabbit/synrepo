use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use synrepo::surface::context::ContextRecipe;

use super::super::bench_shared::{BenchTarget, BenchTask, KNOWN_CATEGORIES};

pub(crate) const SCHEMA_VERSION: u32 = 2;
pub(crate) const BASELINE_KIND_RAW_FILE: &str = "raw_file";

#[derive(Debug, Serialize)]
pub(crate) struct BenchContextReport {
    pub(crate) schema_version: u32,
    pub(crate) summary: BenchContextSummary,
    pub(crate) tasks: Vec<BenchTaskReport>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BenchContextSummary {
    pub(crate) total_tasks: usize,
    pub(crate) tasks_with_hits: usize,
    pub(crate) tasks_with_misses: usize,
    pub(crate) categories: Vec<String>,
    pub(crate) missing_categories: Vec<String>,
    pub(crate) strategy_totals: BTreeMap<String, BenchStrategySummary>,
    pub(crate) ask_improved_tasks: usize,
    pub(crate) ask_matched_tasks: usize,
    pub(crate) ask_regressed_tasks: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct BenchRunSet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) raw_file: Option<BenchStrategyRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lexical: Option<BenchStrategyRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cards: Option<BenchStrategyRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ask: Option<BenchStrategyRun>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BenchTaskReport {
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected_recipe: Option<ContextRecipe>,
    pub(crate) baseline_kind: String,
    pub(crate) raw_file_tokens: usize,
    pub(crate) card_tokens: usize,
    /// `(raw - card) / raw`; zero when `raw_file_tokens` is zero.
    pub(crate) reduction_ratio: f64,
    /// Compatibility alias for the cards run.
    pub(crate) target_hit: bool,
    pub(crate) target_hits: Vec<BenchTarget>,
    pub(crate) target_misses: Vec<BenchTarget>,
    pub(crate) stale_rate: f64,
    pub(crate) latency_ms: u64,
    pub(crate) returned_targets: Vec<String>,
    pub(crate) runs: BenchRunSet,
}

impl BenchTaskReport {
    pub(crate) fn from_fixture(
        fixture_path: &Path,
        fixture: &BenchTask,
        runs: BenchRunSet,
    ) -> anyhow::Result<Self> {
        let cards = runs
            .cards
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("cards run is required for compatibility aliases"))?;
        let reduction_ratio = if cards.raw_file_tokens > 0 {
            cards.raw_file_tokens.saturating_sub(cards.tokens_returned) as f64
                / cards.raw_file_tokens as f64
        } else {
            0.0
        };
        Ok(Self {
            name: fixture
                .name
                .clone()
                .unwrap_or_else(|| fixture_path.display().to_string()),
            category: fixture.category.clone(),
            query: fixture.query.clone(),
            expected_recipe: fixture.expected_recipe,
            baseline_kind: BASELINE_KIND_RAW_FILE.to_string(),
            raw_file_tokens: cards.raw_file_tokens,
            card_tokens: cards.tokens_returned,
            reduction_ratio,
            target_hit: cards.target_hit,
            target_hits: cards.target_hits.clone(),
            target_misses: cards.target_misses.clone(),
            stale_rate: cards.stale_rate,
            latency_ms: cards.latency_ms,
            returned_targets: cards.returned_targets.clone(),
            runs,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BenchStrategyRun {
    pub(crate) task_success: bool,
    pub(crate) tokens_returned: usize,
    pub(crate) tool_calls_needed: usize,
    pub(crate) estimated_followup_files: usize,
    pub(crate) latency_ms: u64,
    pub(crate) citation_coverage: f64,
    pub(crate) span_coverage: f64,
    pub(crate) wrong_context_rate: Option<f64>,
    pub(crate) target_hit: bool,
    pub(crate) target_hits: Vec<BenchTarget>,
    pub(crate) target_misses: Vec<BenchTarget>,
    pub(crate) stale_rate: f64,
    pub(crate) returned_targets: Vec<String>,
    pub(crate) returned_symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected_recipe_hit: Option<bool>,
    #[serde(skip)]
    pub(crate) raw_file_tokens: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct BenchStrategySummary {
    pub(crate) tasks: usize,
    pub(crate) task_success_rate: Option<f64>,
    pub(crate) tokens_returned: usize,
    pub(crate) latency_ms: u64,
    pub(crate) tool_calls_needed: usize,
    pub(crate) estimated_followup_files: usize,
}

pub(crate) fn summarize(tasks: &[BenchTaskReport]) -> BenchContextSummary {
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

    let mut totals = StrategyTotals::default();
    let mut ask_improved_tasks = 0;
    let mut ask_matched_tasks = 0;
    let mut ask_regressed_tasks = 0;
    for task in tasks {
        totals.add("cards", task.runs.cards.as_ref());
        totals.add("ask", task.runs.ask.as_ref());
        totals.add("raw_file", task.runs.raw_file.as_ref());
        totals.add("lexical", task.runs.lexical.as_ref());
        if let (Some(cards), Some(ask)) = (&task.runs.cards, &task.runs.ask) {
            match (cards.task_success, ask.task_success) {
                (false, true) => ask_improved_tasks += 1,
                (true, false) => ask_regressed_tasks += 1,
                _ => ask_matched_tasks += 1,
            }
        }
    }

    BenchContextSummary {
        total_tasks,
        tasks_with_hits,
        tasks_with_misses,
        categories,
        missing_categories,
        strategy_totals: totals.finish(),
        ask_improved_tasks,
        ask_matched_tasks,
        ask_regressed_tasks,
    }
}

#[derive(Default)]
struct StrategyTotals {
    inner: BTreeMap<String, StrategyAccumulator>,
}

impl StrategyTotals {
    fn add(&mut self, name: &str, run: Option<&BenchStrategyRun>) {
        let Some(run) = run else {
            return;
        };
        let entry = self.inner.entry(name.to_string()).or_default();
        entry.tasks += 1;
        entry.successes += usize::from(run.task_success);
        entry.tokens_returned += run.tokens_returned;
        entry.latency_ms = entry.latency_ms.saturating_add(run.latency_ms);
        entry.tool_calls_needed = entry
            .tool_calls_needed
            .saturating_add(run.tool_calls_needed);
        entry.estimated_followup_files = entry
            .estimated_followup_files
            .saturating_add(run.estimated_followup_files);
    }

    fn finish(self) -> BTreeMap<String, BenchStrategySummary> {
        self.inner
            .into_iter()
            .map(|(name, acc)| {
                let success_rate =
                    (acc.tasks > 0).then_some(acc.successes as f64 / acc.tasks as f64);
                (
                    name,
                    BenchStrategySummary {
                        tasks: acc.tasks,
                        task_success_rate: success_rate,
                        tokens_returned: acc.tokens_returned,
                        latency_ms: acc.latency_ms,
                        tool_calls_needed: acc.tool_calls_needed,
                        estimated_followup_files: acc.estimated_followup_files,
                    },
                )
            })
            .collect()
    }
}

#[derive(Default)]
struct StrategyAccumulator {
    tasks: usize,
    successes: usize,
    tokens_returned: usize,
    latency_ms: u64,
    tool_calls_needed: usize,
    estimated_followup_files: usize,
}
