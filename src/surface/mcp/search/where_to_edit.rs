use std::collections::HashSet;
use std::time::Instant;

use serde_json::{json, Value};
use syntext::SearchOptions;

use crate::surface::card::{Budget, CardCompiler};

use super::super::card_set::apply_card_set_cap;
use super::super::limits::DEFAULT_RESPONSE_TOKEN_CAP;
use super::SynrepoState;

mod fallback;
mod recommend;

const MAX_QUERY_ATTEMPTS: usize = 24;
const MAX_MATCHES_PER_QUERY: usize = 50;

#[derive(Debug)]
struct QueryAttempt {
    query: String,
    result_count: usize,
}

pub fn handle_where_to_edit(
    state: &SynrepoState,
    task: String,
    limit: u32,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let result: anyhow::Result<serde_json::Value> = (|| {
        let routing = find_candidate_matches(state, &task, limit)?;
        let query_attempts = routing.query_attempts;
        let fallback_used = routing.fallback_used;
        let matches = routing.matches;

        let (mut cards, matched_index_rows, suggestion_targets) = state
            .with_read_compiler(|compiler| {
                let mut seen = HashSet::new();
                let mut cards = Vec::new();
                let mut suggestion_targets = Vec::new();
                let mut matched_index_rows = 0usize;

                for matched in matches {
                    matched_index_rows += matched.result_count;
                    let key = format!("{}\0{}", matched.root_id, matched.path);
                    if !seen.insert(key) {
                        continue;
                    }

                    if let Some(file) = compiler
                        .reader()
                        .file_by_root_path(&matched.root_id, &matched.path)?
                    {
                        let card = compiler.file_card(file.id, Budget::Tiny)?;
                        let value = serde_json::to_value(&card)
                            .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?;
                        cards.push(value);
                        suggestion_targets.push(matched.path.clone());
                    }

                    if cards.len() >= limit as usize {
                        break;
                    }
                }

                Ok((cards, matched_index_rows, suggestion_targets))
            })
            .map_err(|e| anyhow::anyhow!(e))?;

        let budget_tokens = budget_tokens.or(Some(DEFAULT_RESPONSE_TOKEN_CAP));
        let (truncation_applied, accountings) = apply_card_set_cap(&mut cards, budget_tokens);
        let omitted = omitted_suggestions(&suggestion_targets, cards.len());
        let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
        let latency_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        crate::pipeline::context_metrics::record_cards_best_effort(
            &synrepo_dir,
            &accountings,
            latency_ms,
            false,
        );

        let cards_empty = cards.is_empty();
        let miss_reason = miss_reason(cards_empty, matched_index_rows);
        let recommended_next_queries = recommend::recommended_next_queries(&task, miss_reason);
        let recommended_tool = if recommended_next_queries.is_empty() {
            None
        } else {
            Some("synrepo_search")
        };

        Ok(json!({
            "task": task,
            "suggestions": cards,
            "truncation_applied": truncation_applied,
            "omitted": omitted,
            "query_attempts": query_attempts_json(&query_attempts),
            "fallback_used": fallback_used,
            "miss_reason": miss_reason,
            "recommended_next_queries": recommended_next_queries,
            "recommended_tool": recommended_tool,
        }))
    })();
    super::render_result(result)
}

#[derive(Debug)]
struct RoutingMatches {
    matches: Vec<RoutingMatch>,
    query_attempts: Vec<QueryAttempt>,
    fallback_used: bool,
}

#[derive(Debug)]
struct RoutingMatch {
    path: String,
    root_id: String,
    result_count: usize,
    query_hits: usize,
}

fn find_candidate_matches(
    state: &SynrepoState,
    task: &str,
    limit: u32,
) -> anyhow::Result<RoutingMatches> {
    let original = task.trim();
    let mut query_attempts = Vec::new();
    let mut matches = Vec::new();

    if original.is_empty() {
        return Ok(RoutingMatches {
            matches,
            query_attempts,
            fallback_used: false,
        });
    }

    let original_matches = search_task_query(state, original)?;
    query_attempts.push(QueryAttempt {
        query: original.to_string(),
        result_count: original_matches.len(),
    });

    if !original_matches.is_empty() {
        push_unique_match_paths(&mut matches, original_matches);
        rank_matches(&mut matches);
        return Ok(RoutingMatches {
            matches,
            query_attempts,
            fallback_used: false,
        });
    }

    let mut fallback_used = false;
    for query in fallback::fallback_queries(original) {
        if query_attempts.len() >= MAX_QUERY_ATTEMPTS {
            break;
        }
        fallback_used = true;
        let found = search_task_query(state, &query)?;
        query_attempts.push(QueryAttempt {
            query,
            result_count: found.len(),
        });
        push_unique_match_paths(&mut matches, found);
        if matches.len() >= limit as usize {
            break;
        }
    }
    rank_matches(&mut matches);

    Ok(RoutingMatches {
        matches,
        query_attempts,
        fallback_used,
    })
}

fn search_task_query(
    state: &SynrepoState,
    query: &str,
) -> anyhow::Result<Vec<crate::substrate::RootedSearchMatch>> {
    let options = SearchOptions {
        max_results: Some(MAX_MATCHES_PER_QUERY),
        case_insensitive: true,
        ..SearchOptions::default()
    };
    Ok(crate::substrate::search_rooted_with_options(
        &state.config,
        &state.repo_root,
        query,
        &options,
    )?)
}

fn push_unique_match_paths(
    matches: &mut Vec<RoutingMatch>,
    found: Vec<crate::substrate::RootedSearchMatch>,
) {
    let count = found.len();
    let mut seen_in_query = HashSet::new();
    for m in found {
        let path = m.path.to_string_lossy().to_string();
        let root_id = m.root_id;
        if seen_in_query.insert(format!("{root_id}\0{path}")) {
            if let Some(existing) = matches
                .iter_mut()
                .find(|m| m.root_id == root_id && m.path == path)
            {
                existing.query_hits += 1;
                existing.result_count += count;
                continue;
            }
            matches.push(RoutingMatch {
                path,
                root_id,
                result_count: count,
                query_hits: 1,
            });
        }
    }
}

fn rank_matches(matches: &mut [RoutingMatch]) {
    matches.sort_by(|a, b| {
        crate::surface::query_terms::score_path_for_query(&b.path, b.query_hits)
            .cmp(&crate::surface::query_terms::score_path_for_query(
                &a.path,
                a.query_hits,
            ))
            .then_with(|| a.path.cmp(&b.path))
    });
}

fn omitted_suggestions(targets: &[String], returned_count: usize) -> Vec<Value> {
    targets
        .iter()
        .skip(returned_count)
        .map(|target| {
            json!({
                "target": target,
                "reason": "budget_tokens_exceeded",
            })
        })
        .collect()
}

fn query_attempts_json(attempts: &[QueryAttempt]) -> Vec<Value> {
    attempts
        .iter()
        .map(|attempt| {
            json!({
                "query": attempt.query,
                "result_count": attempt.result_count,
            })
        })
        .collect()
}

fn miss_reason(cards_empty: bool, matched_index_rows: usize) -> Option<&'static str> {
    if !cards_empty {
        return None;
    }
    if matched_index_rows == 0 {
        Some("no_index_matches")
    } else {
        Some("matches_not_in_graph")
    }
}
