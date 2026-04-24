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

use std::path::{Path, PathBuf};
use std::time::Instant;

use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use synrepo::config::Config;
use synrepo::surface::card::{Budget, CardCompiler};
use walkdir::WalkDir;

use super::super::mcp_runtime::prepare_state;

/// Current benchmark report schema version.
///
/// Bump this only when a field is renamed or removed in a way that breaks
/// consumers. Additive field changes keep the same version.
const SCHEMA_VERSION: u32 = 1;

/// Known workflow categories for benchmark fixtures.
///
/// Fixtures may use other strings, but a release-grade benchmark set should
/// cover more than one of these so numeric savings claims are not based on a
/// single happy path.
const KNOWN_CATEGORIES: &[&str] = &[
    "route_to_edit",
    "symbol_explanation",
    "impact_or_risk",
    "test_surface",
];

/// Valid `kind` values for `required_targets` entries.
const KNOWN_TARGET_KINDS: &[&str] = &["file", "symbol", "test"];

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

fn validate_fixture(fixture: &BenchTask) -> anyhow::Result<()> {
    if fixture.query.trim().is_empty() {
        anyhow::bail!("fixture `query` must be non-empty");
    }
    if fixture.category.trim().is_empty() {
        anyhow::bail!("fixture `category` must be non-empty");
    }
    for (idx, target) in fixture.required_targets.iter().enumerate() {
        if target.value.trim().is_empty() {
            anyhow::bail!("required_targets[{idx}]: `value` must be non-empty");
        }
        if !KNOWN_TARGET_KINDS.contains(&target.kind.as_str()) {
            anyhow::bail!(
                "required_targets[{idx}]: unknown `kind` `{}` (expected one of {})",
                target.kind,
                KNOWN_TARGET_KINDS.join(", ")
            );
        }
    }
    Ok(())
}

fn classify_targets(
    required: &[BenchTarget],
    returned_paths: &[String],
    returned_symbols: &[String],
) -> (Vec<BenchTarget>, Vec<BenchTarget>) {
    let mut hits = Vec::new();
    let mut misses = Vec::new();
    for target in required {
        if target_satisfied(target, returned_paths, returned_symbols) {
            hits.push(target.clone());
        } else {
            misses.push(target.clone());
        }
    }
    (hits, misses)
}

fn target_satisfied(
    target: &BenchTarget,
    returned_paths: &[String],
    returned_symbols: &[String],
) -> bool {
    match target.kind.as_str() {
        // Symbol kind prefers matching a qualified name, but falls back to
        // path substring so fixtures stay useful before symbol-level search
        // lands in the card pipeline.
        "symbol" => {
            returned_symbols
                .iter()
                .any(|got| got.contains(&target.value))
                || path_contains(returned_paths, &target.value)
        }
        // `file` and `test` match against returned card paths.
        _ => path_contains(returned_paths, &target.value),
    }
}

fn path_contains(returned_paths: &[String], needle: &str) -> bool {
    returned_paths.iter().any(|got| got.contains(needle))
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

fn expand_task_glob(repo_root: &Path, pattern: &str) -> anyhow::Result<Vec<PathBuf>> {
    let pattern_abs = repo_root.join(pattern).to_string_lossy().to_string();
    let glob = Glob::new(&pattern_abs)?;
    let mut builder = GlobSetBuilder::new();
    builder.add(glob);
    let set = builder.build()?;
    // Walk only the glob's fixed prefix. A pattern like `benches/tasks/*.json`
    // otherwise walks every file under the repo including `.git/` and `target/`.
    let walk_root = fixed_prefix(&pattern_abs).unwrap_or_else(|| repo_root.to_path_buf());
    let mut paths = Vec::new();
    for entry in WalkDir::new(&walk_root).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() && set.is_match(entry.path()) {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Return the deepest parent directory of `pattern` that contains no glob
/// metacharacters. For `/root/benches/tasks/*.json` this is `/root/benches/tasks`.
fn fixed_prefix(pattern: &str) -> Option<PathBuf> {
    let cutoff = pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len());
    let head = &pattern[..cutoff];
    head.rfind('/').map(|idx| PathBuf::from(&head[..idx]))
}

#[derive(Debug, Deserialize)]
struct BenchTask {
    #[serde(default)]
    name: Option<String>,
    category: String,
    query: String,
    #[serde(default)]
    required_targets: Vec<BenchTarget>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct BenchTarget {
    kind: String,
    value: String,
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
