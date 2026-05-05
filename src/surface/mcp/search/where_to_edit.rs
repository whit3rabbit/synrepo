use std::collections::HashSet;
use std::time::Instant;

use serde_json::{json, Value};
use syntext::SearchOptions;

use crate::surface::card::{Budget, CardCompiler};

use super::super::card_set::apply_card_set_cap;
use super::SynrepoState;

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
        let compiler = state
            .create_read_compiler()
            .map_err(|e| anyhow::anyhow!(e))?;
        let mut seen = HashSet::new();
        let mut cards = Vec::new();
        let mut matched_index_rows = 0usize;

        for (path, result_count) in routing.matches {
            matched_index_rows += result_count;
            if !seen.insert(path.clone()) {
                continue;
            }

            if let Some(file) = compiler.reader().file_by_path(&path)? {
                let card = compiler.file_card(file.id, Budget::Tiny)?;
                cards.push(serde_json::to_value(&card)?);
            }

            if cards.len() >= limit as usize {
                break;
            }
        }

        let (truncation_applied, accountings) = apply_card_set_cap(&mut cards, budget_tokens);
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

        Ok(json!({
            "task": task,
            "suggestions": cards,
            "truncation_applied": truncation_applied,
            "query_attempts": query_attempts_json(&routing.query_attempts),
            "fallback_used": routing.fallback_used,
            "miss_reason": miss_reason,
        }))
    })();
    super::render_result(result)
}

#[derive(Debug)]
struct RoutingMatches {
    matches: Vec<(String, usize)>,
    query_attempts: Vec<QueryAttempt>,
    fallback_used: bool,
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
        return Ok(RoutingMatches {
            matches,
            query_attempts,
            fallback_used: false,
        });
    }

    let mut fallback_used = false;
    for query in fallback_queries(original) {
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

    Ok(RoutingMatches {
        matches,
        query_attempts,
        fallback_used,
    })
}

fn search_task_query(
    state: &SynrepoState,
    query: &str,
) -> anyhow::Result<Vec<syntext::SearchMatch>> {
    let options = SearchOptions {
        max_results: Some(MAX_MATCHES_PER_QUERY),
        case_insensitive: true,
        ..SearchOptions::default()
    };
    Ok(crate::substrate::search_with_options(
        &state.config,
        &state.repo_root,
        query,
        &options,
    )?)
}

fn push_unique_match_paths(matches: &mut Vec<(String, usize)>, found: Vec<syntext::SearchMatch>) {
    let count = found.len();
    let mut seen_in_query = HashSet::new();
    for m in found {
        let path = m.path.to_string_lossy().to_string();
        if seen_in_query.insert(path.clone()) {
            matches.push((path, count));
        }
    }
}

fn fallback_queries(task: &str) -> Vec<String> {
    let tokens = task_tokens(task);
    let mut queries = Vec::new();
    let mut seen = HashSet::new();

    for width in 2..=3 {
        for window in tokens.windows(width) {
            add_query_variants(&mut queries, &mut seen, window);
        }
    }

    for token in &tokens {
        add_candidate(&mut queries, &mut seen, token.clone());
        if let Some(singular) = singularize(token) {
            add_candidate(&mut queries, &mut seen, singular);
        }
    }

    queries
}

fn add_query_variants(queries: &mut Vec<String>, seen: &mut HashSet<String>, window: &[String]) {
    let snake = window.join("_");
    if let Some(plural) = pluralize_phrase_tail(&snake) {
        add_candidate(queries, seen, plural);
    }
    add_candidate(queries, seen, snake.clone());

    let phrase = window.join(" ");
    if let Some(plural) = pluralize_phrase_tail(&phrase) {
        add_candidate(queries, seen, plural);
    }
    add_candidate(queries, seen, phrase.clone());
}

fn add_candidate(queries: &mut Vec<String>, seen: &mut HashSet<String>, query: String) {
    if query.len() < 3 {
        return;
    }
    if seen.insert(query.clone()) {
        queries.push(query);
    }
}

fn task_tokens(task: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in task.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else {
            push_token(&mut tokens, &mut current);
        }
    }
    push_token(&mut tokens, &mut current);
    tokens
}

fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.is_empty() {
        return;
    }
    let token = std::mem::take(current);
    if !is_stopword(&token) {
        tokens.push(singularize(&token).unwrap_or(token));
    }
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "for"
            | "from"
            | "in"
            | "into"
            | "is"
            | "it"
            | "of"
            | "on"
            | "or"
            | "our"
            | "that"
            | "the"
            | "this"
            | "to"
            | "with"
    )
}

fn singularize(token: &str) -> Option<String> {
    if token.len() > 4 && token.ends_with("ies") {
        return Some(format!("{}y", &token[..token.len() - 3]));
    }
    if token.len() > 3 && token.ends_with('s') && !token.ends_with("ss") {
        return Some(token[..token.len() - 1].to_string());
    }
    None
}

fn pluralize_phrase_tail(phrase: &str) -> Option<String> {
    if phrase.ends_with('s') {
        return None;
    }
    Some(format!("{phrase}s"))
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

#[cfg(test)]
mod tests {
    use super::{fallback_queries, task_tokens};

    #[test]
    fn fallback_queries_include_snake_case_plural_forms() {
        let queries = fallback_queries("agent hook routing with context metrics");

        assert!(queries.iter().any(|q| q == "agent_hooks"));
        assert!(queries.iter().any(|q| q == "context_metrics"));
    }

    #[test]
    fn task_tokens_drop_filler_and_singularize() {
        assert_eq!(
            task_tokens("extend the hooks with structured signals"),
            vec!["extend", "hook", "structured", "signal"]
        );
    }
}
