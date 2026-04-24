use std::path::{Path, PathBuf};
use std::time::Instant;

use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use synrepo::config::Config;
use synrepo::surface::card::{Budget, CardCompiler};
use synrepo::surface::mcp::{cards, search};
use walkdir::WalkDir;

use super::mcp_runtime::prepare_state;

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

pub(crate) fn stats_context(repo_root: &Path, json_output: bool) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let metrics = synrepo::pipeline::context_metrics::load(&synrepo_dir)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&metrics)?);
    } else {
        println!("synrepo context stats");
        println!("  cards served: {}", metrics.cards_served_total);
        println!("  avg tokens/card: {:.1}", metrics.card_tokens_avg());
        println!(
            "  est. tokens avoided: {}",
            metrics.estimated_tokens_saved_total
        );
    }
    Ok(())
}

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
        let fixture: BenchTask = serde_json::from_slice(&std::fs::read(&path)?)?;
        let start = Instant::now();
        let matches = synrepo::substrate::search(&config, repo_root, &fixture.query)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut card_tokens = 0usize;
        let mut raw_tokens = 0usize;
        let mut returned_targets = Vec::new();

        for m in matches.iter().take(10) {
            let rel = m.path.to_string_lossy().to_string();
            if !seen.insert(rel.clone()) {
                continue;
            }
            if let Some(file) = compiler.reader().file_by_path(&rel)? {
                let card = compiler.file_card(file.id, Budget::Tiny)?;
                card_tokens += card.context_accounting.token_estimate;
                raw_tokens += card.context_accounting.raw_file_token_estimate;
                returned_targets.push(card.path);
            }
            if returned_targets.len() >= 5 {
                break;
            }
        }

        let required_targets = fixture.required_targets.unwrap_or_default();
        let target_hit = required_targets
            .iter()
            .all(|required| returned_targets.iter().any(|got| got.contains(required)));
        let reduction_ratio = if raw_tokens > 0 {
            raw_tokens.saturating_sub(card_tokens) as f64 / raw_tokens as f64
        } else {
            0.0
        };
        tasks.push(BenchTaskReport {
            name: fixture.name.unwrap_or_else(|| path.display().to_string()),
            query: fixture.query,
            raw_file_tokens: raw_tokens,
            card_tokens,
            reduction_ratio,
            target_hit,
            latency_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            returned_targets,
        });
    }

    let report = BenchContextReport { tasks };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("context benchmark: {} task(s)", report.tasks.len());
        for task in &report.tasks {
            println!(
                "  {}: {:.1}% reduction, hit={}",
                task.name,
                task.reduction_ratio * 100.0,
                task.target_hit
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

fn expand_task_glob(repo_root: &Path, pattern: &str) -> anyhow::Result<Vec<PathBuf>> {
    let pattern_abs = repo_root.join(pattern).to_string_lossy().to_string();
    let glob = Glob::new(&pattern_abs)?;
    let mut builder = GlobSetBuilder::new();
    builder.add(glob);
    let set = builder.build()?;
    let mut paths = Vec::new();
    for entry in WalkDir::new(repo_root).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() && set.is_match(entry.path()) {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Debug, Deserialize)]
struct BenchTask {
    name: Option<String>,
    query: String,
    required_targets: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct BenchContextReport {
    tasks: Vec<BenchTaskReport>,
}

#[derive(Debug, Serialize)]
struct BenchTaskReport {
    name: String,
    query: String,
    raw_file_tokens: usize,
    card_tokens: usize,
    reduction_ratio: f64,
    target_hit: bool,
    latency_ms: u64,
    returned_targets: Vec<String>,
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

    #[test]
    fn expand_task_glob_finds_json_fixtures() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("benches/tasks")).unwrap();
        std::fs::write(
            dir.path().join("benches/tasks/context.json"),
            r#"{"query":"auth"}"#,
        )
        .unwrap();

        let paths = expand_task_glob(dir.path(), "benches/tasks/*.json").unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("context.json"));
    }
}
