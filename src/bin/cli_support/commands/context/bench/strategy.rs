use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};
use synrepo::config::Config;
use synrepo::surface::card::compiler::GraphCardCompiler;
use synrepo::surface::card::{Budget, CardCompiler};
use synrepo::surface::context::ContextAskRequest;
use synrepo::surface::mcp::response_budget::estimate_json_tokens;
use synrepo::surface::mcp::{ask, SynrepoState};

use super::super::bench_shared::{classify_targets, wrong_context_rate, BenchTask};
use super::report::BenchStrategyRun;

const MAX_SEARCH_MATCHES: usize = 10;
const MAX_RETURNED_CARDS: usize = 5;

pub(crate) fn run_raw_file(
    repo_root: &Path,
    config: &Config,
    compiler: &GraphCardCompiler,
    fixture: &BenchTask,
) -> anyhow::Result<BenchStrategyRun> {
    let start = Instant::now();
    let sample = collect_card_sample(repo_root, config, compiler, fixture)?;
    Ok(build_run(
        RunMetrics {
            tokens_returned: sample.raw_tokens,
            raw_file_tokens: sample.raw_tokens,
            tool_calls_needed: 1 + sample.returned_paths.len(),
            citation_coverage: 0.0,
            span_coverage: 0.0,
            expected_recipe_hit: None,
        },
        start,
        sample,
        fixture,
    ))
}

pub(crate) fn run_lexical(
    repo_root: &Path,
    config: &Config,
    fixture: &BenchTask,
) -> anyhow::Result<BenchStrategyRun> {
    let start = Instant::now();
    let matches = synrepo::substrate::search(config, repo_root, &fixture.query)?;
    let mut rows = Vec::new();
    let mut returned_paths = Vec::new();
    for m in matches.iter().take(MAX_SEARCH_MATCHES) {
        let path = m.path.to_string_lossy().to_string();
        returned_paths.push(path.clone());
        rows.push(json!({
            "path": path,
            "line": m.line_number,
            "preview": String::from_utf8_lossy(&m.line_content).trim_end(),
        }));
    }
    dedup(&mut returned_paths);
    let tokens_returned = estimate_json_tokens(&json!({ "results": rows }));
    let sample = CardSample {
        card_tokens: tokens_returned,
        raw_tokens: 0,
        returned_paths,
        returned_symbols: Vec::new(),
        cards_examined: 0,
        stale_cards: 0,
    };
    Ok(build_run(
        RunMetrics {
            tokens_returned,
            raw_file_tokens: 0,
            tool_calls_needed: 1,
            citation_coverage: 0.0,
            span_coverage: 0.0,
            expected_recipe_hit: None,
        },
        start,
        sample,
        fixture,
    ))
}

pub(crate) fn run_cards(
    repo_root: &Path,
    config: &Config,
    compiler: &GraphCardCompiler,
    fixture: &BenchTask,
) -> anyhow::Result<BenchStrategyRun> {
    let start = Instant::now();
    let sample = collect_card_sample(repo_root, config, compiler, fixture)?;
    Ok(build_run(
        RunMetrics {
            tokens_returned: sample.card_tokens,
            raw_file_tokens: sample.raw_tokens,
            tool_calls_needed: 1 + sample.returned_paths.len(),
            citation_coverage: 0.0,
            span_coverage: 0.0,
            expected_recipe_hit: None,
        },
        start,
        sample,
        fixture,
    ))
}

pub(crate) fn run_ask(
    repo_root: &Path,
    state: &SynrepoState,
    fixture: &BenchTask,
) -> anyhow::Result<BenchStrategyRun> {
    let start = Instant::now();
    let packet = ask::build_ask_packet(state, ask_request(repo_root, fixture))?;
    let tokens_returned = estimate_json_tokens(&packet);
    let (returned_paths, returned_symbols) = returned_from_ask_packet(&packet);
    let evidence = packet
        .get("evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let citation_coverage = citation_coverage(&packet, evidence.len(), fixture);
    let span_coverage = span_coverage(&evidence);
    let expected_recipe_hit = fixture.expected_recipe.map(|expected| {
        packet.get("recipe") == Some(&serde_json::to_value(expected).unwrap_or(Value::Null))
    });
    let sample = CardSample {
        card_tokens: tokens_returned,
        raw_tokens: ask_raw_tokens(&packet),
        returned_paths,
        returned_symbols,
        cards_examined: 0,
        stale_cards: 0,
    };
    Ok(build_run(
        RunMetrics {
            tokens_returned,
            raw_file_tokens: sample.raw_tokens,
            tool_calls_needed: 1,
            citation_coverage,
            span_coverage,
            expected_recipe_hit,
        },
        start,
        sample,
        fixture,
    ))
}

fn ask_request(repo_root: &Path, fixture: &BenchTask) -> ContextAskRequest {
    ContextAskRequest {
        repo_root: Some(repo_root.to_path_buf()),
        ask: fixture.query.clone(),
        scope: fixture.scope.clone().unwrap_or_default(),
        shape: fixture.shape.clone().unwrap_or_default(),
        ground: fixture.ground.clone().unwrap_or_default(),
        budget: fixture.budget.clone().unwrap_or_default(),
    }
}

struct RunMetrics {
    tokens_returned: usize,
    raw_file_tokens: usize,
    tool_calls_needed: usize,
    citation_coverage: f64,
    span_coverage: f64,
    expected_recipe_hit: Option<bool>,
}

fn build_run(
    metrics: RunMetrics,
    start: Instant,
    mut sample: CardSample,
    fixture: &BenchTask,
) -> BenchStrategyRun {
    dedup(&mut sample.returned_paths);
    dedup(&mut sample.returned_symbols);
    let (target_hits, target_misses) = classify_targets(
        &fixture.required_targets,
        &sample.returned_paths,
        &sample.returned_symbols,
    );
    let target_hit = target_misses.is_empty();
    let recipe_ok = metrics.expected_recipe_hit.unwrap_or(true);
    let stale_rate = if sample.cards_examined > 0 {
        sample.stale_cards as f64 / sample.cards_examined as f64
    } else {
        0.0
    };
    BenchStrategyRun {
        task_success: target_hit && recipe_ok,
        tokens_returned: metrics.tokens_returned,
        tool_calls_needed: metrics.tool_calls_needed,
        estimated_followup_files: target_misses.len(),
        latency_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        citation_coverage: metrics.citation_coverage,
        span_coverage: metrics.span_coverage,
        wrong_context_rate: wrong_context_rate(
            fixture.allowed_context.as_deref(),
            &sample.returned_paths,
            &sample.returned_symbols,
        ),
        target_hit,
        target_hits,
        target_misses,
        stale_rate,
        returned_targets: sample.returned_paths,
        returned_symbols: sample.returned_symbols,
        expected_recipe_hit: metrics.expected_recipe_hit,
        raw_file_tokens: metrics.raw_file_tokens,
    }
}

fn collect_card_sample(
    repo_root: &Path,
    config: &Config,
    compiler: &GraphCardCompiler,
    fixture: &BenchTask,
) -> anyhow::Result<CardSample> {
    let matches = synrepo::substrate::search(config, repo_root, &fixture.query)?;
    let mut seen = BTreeSet::new();
    let mut sample = CardSample::default();
    for m in matches.iter().take(MAX_SEARCH_MATCHES) {
        let rel = m.path.to_string_lossy().to_string();
        if !seen.insert(rel.clone()) {
            continue;
        }
        if let Some(file) = compiler.reader().file_by_path(&rel)? {
            let card = compiler.file_card(file.id, Budget::Tiny)?;
            sample.cards_examined += 1;
            sample.stale_cards += usize::from(card.context_accounting.stale);
            sample.card_tokens += card.context_accounting.token_estimate;
            sample.raw_tokens += card.context_accounting.raw_file_token_estimate;
            sample.returned_paths.push(card.path);
            for sym in &card.symbols {
                sample.returned_symbols.push(sym.qualified_name.clone());
            }
        }
        if sample.returned_paths.len() >= MAX_RETURNED_CARDS {
            break;
        }
    }
    Ok(sample)
}

#[derive(Default)]
struct CardSample {
    card_tokens: usize,
    raw_tokens: usize,
    returned_paths: Vec<String>,
    returned_symbols: Vec<String>,
    cards_examined: usize,
    stale_cards: usize,
}

fn returned_from_ask_packet(packet: &Value) -> (Vec<String>, Vec<String>) {
    let mut paths = Vec::new();
    let mut symbols = Vec::new();
    collect_evidence_sources(packet, &mut paths);
    if let Some(artifacts) = packet
        .pointer("/context_packet/artifacts")
        .and_then(Value::as_array)
    {
        for artifact in artifacts {
            collect_artifact_targets(artifact, &mut paths, &mut symbols);
        }
    }
    dedup(&mut paths);
    dedup(&mut symbols);
    (paths, symbols)
}

fn collect_evidence_sources(packet: &Value, paths: &mut Vec<String>) {
    let Some(evidence) = packet.get("evidence").and_then(Value::as_array) else {
        return;
    };
    for item in evidence {
        if let Some(source) = item.get("source").and_then(Value::as_str) {
            push_path_like(paths, source);
        }
    }
}

fn collect_artifact_targets(artifact: &Value, paths: &mut Vec<String>, symbols: &mut Vec<String>) {
    if artifact.get("status").and_then(Value::as_str) != Some("ok") {
        return;
    }
    let target_kind = artifact.get("target_kind").and_then(Value::as_str);
    let artifact_type = artifact.get("artifact_type").and_then(Value::as_str);
    if let Some(target) = artifact.get("target").and_then(Value::as_str) {
        if target_kind == Some("symbol") || artifact_type == Some("symbol") {
            symbols.push(target.to_string());
        } else if target_kind != Some("search") && artifact_type != Some("search") {
            paths.push(target.to_string());
        }
    }
    let content = artifact.get("content").unwrap_or(&Value::Null);
    if let Some(path) = content.get("path").and_then(Value::as_str) {
        paths.push(path.to_string());
    }
    if let Some(defined_at) = content.get("defined_at").and_then(Value::as_str) {
        if let Some((path, _)) = defined_at.rsplit_once(':') {
            paths.push(path.to_string());
        }
    }
    if let Some(qname) = content.get("qualified_name").and_then(Value::as_str) {
        symbols.push(qname.to_string());
    }
    collect_search_targets(content, paths);
}

fn collect_search_targets(content: &Value, paths: &mut Vec<String>) {
    if let Some(rows) = content.get("results").and_then(Value::as_array) {
        for row in rows {
            if let Some(path) = row.get("path").and_then(Value::as_str) {
                push_path_like(paths, path);
            }
        }
    }
    if let Some(groups) = content.get("file_groups").and_then(Value::as_array) {
        for group in groups {
            if let Some(path) = group.get("path").and_then(Value::as_str) {
                push_path_like(paths, path);
            }
        }
    }
}

fn push_path_like(paths: &mut Vec<String>, value: &str) {
    if value.contains('/') || value.contains('.') {
        paths.push(value.to_string());
    }
}

fn citation_coverage(packet: &Value, evidence_count: usize, fixture: &BenchTask) -> f64 {
    let cards_used = packet
        .get("cards_used")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let denominator = cards_used.max(fixture.required_targets.len()).max(1);
    (evidence_count as f64 / denominator as f64).min(1.0)
}

fn span_coverage(evidence: &[Value]) -> f64 {
    if evidence.is_empty() {
        return 0.0;
    }
    let spans = evidence
        .iter()
        .filter(|item| item.get("span").is_some_and(Value::is_object))
        .count();
    spans as f64 / evidence.len() as f64
}

fn ask_raw_tokens(packet: &Value) -> usize {
    packet
        .pointer("/context_packet/context_state/raw_file_token_estimate")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
}

fn dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}
