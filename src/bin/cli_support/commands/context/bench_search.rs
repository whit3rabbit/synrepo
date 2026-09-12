//! `synrepo bench search` — fixture-backed lexical vs hybrid search eval.

mod report;

#[cfg(test)]
use report::BenchSearchSummary;
use report::{render_report, summarize, BenchSearchReport, BenchSearchRun, BenchSearchTaskReport};

use std::path::Path;
use std::time::Instant;

use synrepo::core::ids::SymbolNodeId;
use synrepo::substrate::{
    dense_first_search, hybrid_search, HybridSearchReport, HybridSearchRow, HybridSearchSource,
};
use synrepo::surface::card::compiler::GraphCardCompiler;
use syntext::SearchOptions;

use super::super::mcp_runtime::prepare_state;
use super::bench_shared::{
    classify_targets, expand_task_glob, validate_fixture, BenchTarget, BenchTask,
};

const SCHEMA_VERSION: u32 = 1;
const HIT_LIMIT: usize = 5;
const SEARCH_FETCH_LIMIT: usize = HIT_LIMIT * 2;
const FIXTURE_PATH_PREFIX: &str = "benches/";

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
        let dense_first = if mode.includes_dense_first() {
            Some(run_dense_first(repo_root, &config, &compiler, &fixture)?)
        } else {
            None
        };
        tasks.push(BenchSearchTaskReport {
            name: fixture.name.unwrap_or_else(|| path.display().to_string()),
            category: fixture.category,
            query: fixture.query,
            lexical,
            auto,
            dense_first,
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
    let mut returned_targets = Vec::new();
    for m in matches {
        let path = m.path.to_string_lossy().to_string();
        if is_benchmark_fixture_path(&path) {
            continue;
        }
        returned_targets.push(path);
        if returned_targets.len() >= HIT_LIMIT {
            break;
        }
    }
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

fn run_arm<F>(
    repo_root: &Path,
    config: &synrepo::config::Config,
    compiler: &GraphCardCompiler,
    fixture: &BenchTask,
    counts_non_lexical: bool,
    run_search: F,
) -> anyhow::Result<BenchSearchRun>
where
    F: Fn(&synrepo::config::Config, &Path, &str) -> anyhow::Result<HybridSearchReport>,
{
    let start = Instant::now();
    let report = run_search(config, repo_root, &fixture.query)?;
    let semantic_row_count = if counts_non_lexical {
        report
            .rows
            .iter()
            .filter(|row| row.source != HybridSearchSource::Lexical)
            .count()
    } else {
        // Dense-first emits lexical and semantic sources but never the
        // `Hybrid` source from auto's RRF fusion. Count semantic rows so
        // the field still reflects the model's contribution.
        report
            .rows
            .iter()
            .filter(|row| row.source == HybridSearchSource::Semantic)
            .count()
    };
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

fn run_auto(
    repo_root: &Path,
    config: &synrepo::config::Config,
    compiler: &GraphCardCompiler,
    fixture: &BenchTask,
) -> anyhow::Result<BenchSearchRun> {
    run_arm(repo_root, config, compiler, fixture, true, |c, r, q| {
        hybrid_search(c, r, q, &search_options()).map_err(anyhow::Error::from)
    })
}

fn run_dense_first(
    repo_root: &Path,
    config: &synrepo::config::Config,
    compiler: &GraphCardCompiler,
    fixture: &BenchTask,
) -> anyhow::Result<BenchSearchRun> {
    run_arm(repo_root, config, compiler, fixture, false, |c, r, q| {
        dense_first_search(c, r, q, &search_options()).map_err(anyhow::Error::from)
    })
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
            if !is_benchmark_fixture_path(&path) {
                targets.push(path);
            }
        }
        if let Some(symbol_id) = row.symbol_id {
            if let Some((path, qname)) = symbol_details(compiler, symbol_id) {
                if !is_benchmark_fixture_path(&path) {
                    targets.push(path);
                    symbols.push(qname);
                }
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
    let mut options = SearchOptions::default();
    options.max_results = Some(SEARCH_FETCH_LIMIT);
    options
}

fn is_benchmark_fixture_path(value: &str) -> bool {
    value.starts_with(FIXTURE_PATH_PREFIX)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BenchSearchMode {
    Lexical,
    Auto,
    DenseFirst,
    /// Lexical + auto arms. (Default for `--mode both`.)
    Both,
    /// Lexical + auto + dense-first. Use to read off the dense-first-vs-RRF
    /// summary fields against the bench.
    All,
}

impl BenchSearchMode {
    fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "lexical" => Ok(Self::Lexical),
            "auto" => Ok(Self::Auto),
            "dense-first" | "dense_first" => Ok(Self::DenseFirst),
            "both" => Ok(Self::Both),
            "all" => Ok(Self::All),
            other => anyhow::bail!(
                "unknown bench search mode `{other}`; expected lexical, auto, dense-first, both, or all"
            ),
        }
    }

    fn includes_lexical(self) -> bool {
        matches!(self, Self::Lexical | Self::Both | Self::All)
    }

    fn includes_auto(self) -> bool {
        matches!(self, Self::Auto | Self::Both | Self::All)
    }

    fn includes_dense_first(self) -> bool {
        matches!(self, Self::DenseFirst | Self::All)
    }
}

#[cfg(test)]
mod tests;
