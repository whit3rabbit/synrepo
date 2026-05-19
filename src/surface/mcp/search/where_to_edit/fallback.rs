use std::collections::HashSet;

pub(super) fn fallback_queries(task: &str) -> Vec<String> {
    let tokens = task_tokens(task);
    let mut queries = Vec::new();
    let mut seen = HashSet::new();

    add_code_shaped_tokens(&mut queries, &mut seen, task);
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

fn add_code_shaped_tokens(queries: &mut Vec<String>, seen: &mut HashSet<String>, task: &str) {
    for raw in task.split_whitespace() {
        let token = raw.trim_matches(|ch: char| {
            !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-' | '/' | '.')
        });
        if token.len() < 3 {
            continue;
        }
        if token.contains('_')
            || token.contains('-')
            || token.contains('/')
            || token.contains('.')
            || has_mixed_case(token)
        {
            add_candidate(queries, seen, token.to_string());
        }
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

    #[test]
    fn lifecycle_review_terms_are_queried_before_phrase_variants() {
        let queries = fallback_queries(
            "Review likely memory/resource leaks and uncaught error handling risks in long-lived server, goroutine, PTY/process, database, and timer lifecycle code",
        );

        let goroutine = queries.iter().position(|q| q == "goroutine").unwrap();
        let review_likely = queries.iter().position(|q| q == "review_likelys");
        assert!(review_likely.is_none());
        assert!(goroutine < 10, "{queries:?}");
        assert!(queries.iter().any(|q| q == "PTY/process"));
        assert!(queries.iter().any(|q| q == "QueryContext"));
    }
}
