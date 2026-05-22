//! Shared natural-language query normalization for task-context routing.

use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueryTerm {
    pub(crate) text: String,
    pub(crate) code_shaped: bool,
}

pub(crate) fn extract_terms(task: &str) -> Vec<QueryTerm> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();

    for raw in task.split_whitespace() {
        let token = trim_query_token(raw);
        if token.len() < 3 {
            continue;
        }
        let code_shaped = is_code_shaped(token);
        if code_shaped {
            push_term(&mut terms, &mut seen, token.to_string(), true);
            for part in split_identifier(token) {
                push_term(&mut terms, &mut seen, part, false);
            }
        }
    }

    for token in natural_tokens(task) {
        push_term(&mut terms, &mut seen, token.clone(), false);
        if let Some(stemmed) = light_stem(&token) {
            push_term(&mut terms, &mut seen, stemmed, false);
        }
    }

    terms
}

pub(crate) fn fallback_queries(task: &str) -> Vec<String> {
    let terms = extract_terms(task);
    let tokens: Vec<String> = terms
        .iter()
        .filter(|term| !term.code_shaped)
        .map(|term| term.text.clone())
        .collect();
    let mut queries = Vec::new();
    let mut seen = HashSet::new();

    for term in terms.iter().filter(|term| term.code_shaped) {
        add_candidate(&mut queries, &mut seen, term.text.clone());
    }
    add_domain_queries(&mut queries, &mut seen, task);

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

pub(crate) fn score_path_for_query(path: &str, query_hits: usize) -> i32 {
    let lower = path.to_ascii_lowercase();
    let mut score = (query_hits as i32) * 20;
    if lower.contains("/test")
        || lower.contains("_test.")
        || lower.contains(".test.")
        || lower.contains("/spec")
        || lower.contains(".spec.")
        || lower.contains("/fixtures/")
        || lower.contains("/examples/")
    {
        score -= 15;
    }
    if lower.starts_with("src/") || lower.contains("/src/") {
        score += 5;
    }
    score
}

fn push_term(
    terms: &mut Vec<QueryTerm>,
    seen: &mut HashSet<String>,
    text: String,
    code_shaped: bool,
) {
    if text.len() < 3 || is_stopword(&text) {
        return;
    }
    let key = text.to_ascii_lowercase();
    if seen.insert(key) {
        terms.push(QueryTerm { text, code_shaped });
    }
}

fn trim_query_token(raw: &str) -> &str {
    raw.trim_matches(|ch: char| {
        !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-' | '/' | '.' | ':')
    })
}

fn is_code_shaped(token: &str) -> bool {
    token.contains('_')
        || token.contains('-')
        || token.contains('/')
        || token.contains('.')
        || token.contains("::")
        || has_mixed_case(token)
        || is_acronym(token)
}

fn split_identifier(token: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in token.split(['_', '-', '/', '.', ':']) {
        if segment.is_empty() {
            continue;
        }
        let mut current = String::new();
        let chars: Vec<char> = segment.chars().collect();
        for (idx, ch) in chars.iter().enumerate() {
            if idx > 0 {
                let prev = chars[idx - 1];
                let next = chars.get(idx + 1).copied();
                if ch.is_ascii_uppercase()
                    && (prev.is_ascii_lowercase() || next.is_some_and(|n| n.is_ascii_lowercase()))
                {
                    push_identifier_part(&mut out, &mut current);
                }
            }
            current.push(ch.to_ascii_lowercase());
        }
        push_identifier_part(&mut out, &mut current);
    }
    out
}

fn push_identifier_part(out: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() && !is_stopword(current) {
        out.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

fn natural_tokens(task: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in task.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else {
            push_natural_token(&mut tokens, &mut current);
        }
    }
    push_natural_token(&mut tokens, &mut current);
    tokens
}

fn push_natural_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.is_empty() {
        return;
    }
    let token = std::mem::take(current);
    if !is_stopword(&token) {
        tokens.push(singularize(&token).unwrap_or(token));
    }
}

fn add_domain_queries(queries: &mut Vec<String>, seen: &mut HashSet<String>, task: &str) {
    let lower = task.to_ascii_lowercase();
    if contains_any(&lower, &["agent hook", "agent hooks"]) {
        add_candidate(queries, seen, "agent_hooks".to_string());
    }
    if contains_any(&lower, &["context metric", "context metrics"]) {
        add_candidate(queries, seen, "context_metrics".to_string());
    }
    if contains_any(
        &lower,
        &[
            "memory",
            "resource leak",
            "goroutine",
            "uncaught",
            "error handling",
            "long-lived",
            "pty",
            "process",
            "database",
            "timer",
            "lifecycle",
        ],
    ) {
        for query in [
            "goroutine",
            "go func",
            "context.With",
            "Close",
            "Wait",
            "exec.Command",
            "pty",
            "time.NewTicker",
            "time.AfterFunc",
            "panic",
            "recover",
            "log.Fatal",
            "os.Exit",
            "QueryContext",
            "Rows",
            "http.Handler",
        ] {
            add_candidate(queries, seen, query.to_string());
        }
    }
}

fn add_query_variants(queries: &mut Vec<String>, seen: &mut HashSet<String>, window: &[String]) {
    let snake = window.join("_");
    if let Some(plural) = pluralize_phrase_tail(&snake) {
        add_candidate(queries, seen, plural);
    }
    add_candidate(queries, seen, snake);

    let phrase = window.join(" ");
    if let Some(plural) = pluralize_phrase_tail(&phrase) {
        add_candidate(queries, seen, plural);
    }
    add_candidate(queries, seen, phrase);
}

fn add_candidate(queries: &mut Vec<String>, seen: &mut HashSet<String>, query: String) {
    if query.len() < 3 {
        return;
    }
    if seen.insert(query.clone()) {
        queries.push(query);
    }
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "add"
            | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "codebase"
            | "for"
            | "from"
            | "in"
            | "into"
            | "is"
            | "it"
            | "likely"
            | "of"
            | "on"
            | "or"
            | "our"
            | "review"
            | "risk"
            | "risks"
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

fn light_stem(token: &str) -> Option<String> {
    if token.len() > 6 && token.ends_with("ing") {
        return Some(token[..token.len() - 3].to_string());
    }
    if token.len() > 5 && token.ends_with("ed") {
        return Some(token[..token.len() - 2].to_string());
    }
    singularize(token)
}

fn pluralize_phrase_tail(phrase: &str) -> Option<String> {
    if phrase.ends_with('s') {
        return None;
    }
    Some(format!("{phrase}s"))
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn has_mixed_case(token: &str) -> bool {
    if is_titlecase_word(token) {
        return false;
    }
    token.chars().any(|ch| ch.is_ascii_lowercase())
        && token.chars().any(|ch| ch.is_ascii_uppercase())
}

fn is_titlecase_word(token: &str) -> bool {
    let mut chars = token.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_lowercase())
}

fn is_acronym(token: &str) -> bool {
    token.len() > 2 && token.chars().all(|ch| ch.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::{extract_terms, fallback_queries, score_path_for_query};

    #[test]
    fn extracts_identifier_parts_and_acronyms() {
        let terms = extract_terms("Find HTTPServer.handle_request in src/api.ts");
        let texts = terms.into_iter().map(|term| term.text).collect::<Vec<_>>();

        assert!(texts.contains(&"HTTPServer.handle_request".to_string()));
        assert!(texts.contains(&"http".to_string()));
        assert!(texts.contains(&"server".to_string()));
        assert!(texts.contains(&"handle".to_string()));
        assert!(texts.contains(&"request".to_string()));
        assert!(texts.contains(&"src/api.ts".to_string()));
    }

    #[test]
    fn fallback_queries_include_domain_and_phrase_forms() {
        let queries = fallback_queries("agent hook routing with context metrics");

        assert!(queries.iter().any(|q| q == "agent_hooks"));
        assert!(queries.iter().any(|q| q == "context_metrics"));
        assert!(queries.iter().any(|q| q == "hook_routings"));
    }

    #[test]
    fn path_score_downranks_tests_and_fixtures() {
        assert!(
            score_path_for_query("src/lib.rs", 2) > score_path_for_query("tests/lib_test.rs", 2)
        );
        assert!(
            score_path_for_query("src/main.rs", 1) > score_path_for_query("examples/main.rs", 1)
        );
    }
}
