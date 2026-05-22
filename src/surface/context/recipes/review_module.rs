use super::{
    is_file_path, path_target_kind, push_searches, push_unique, scoped_paths, scoped_symbols,
};
use crate::surface::context::types::{ContextAskRequest, ContextTarget};

pub(super) fn plan(targets: &mut Vec<ContextTarget>, request: &ContextAskRequest, budget: &str) {
    for path in scoped_paths(request) {
        push_unique(targets, path_target_kind(path), path, Some(budget));
        if is_file_path(path) {
            push_unique(targets, "source_slice", path, Some("normal"));
            push_unique(targets, "minimum_context", path, Some("tiny"));
            push_unique(targets, "change_risk", path, Some("normal"));
        } else {
            push_unique(targets, "public_api", path, Some("normal"));
            push_unique(targets, "entrypoints", path, Some("normal"));
        }
    }
    for symbol in scoped_symbols(request) {
        push_unique(targets, "symbol", symbol, Some(budget));
        push_unique(targets, "source_slice", symbol, Some("normal"));
        push_unique(targets, "minimum_context", symbol, Some("tiny"));
        push_unique(targets, "change_risk", symbol, Some("normal"));
    }
    if targets.is_empty() {
        push_unique(targets, "directory", ".", Some("tiny"));
        push_unique(targets, "public_api", ".", Some("normal"));
        push_unique(targets, "entrypoints", ".", Some("normal"));
        push_searches(targets, review_search_queries(&request.ask));
        push_unique(targets, "search", request.ask.trim(), Some("tiny"));
    }
}

fn review_search_queries(ask: &str) -> Vec<&'static str> {
    let lower = ask.to_ascii_lowercase();
    let mut queries = Vec::new();
    if contains_any(
        &lower,
        &[
            "memory",
            "resource",
            "leak",
            "goroutine",
            "uncaught",
            "error handling",
            "process",
            "pty",
            "database",
            "timer",
            "lifecycle",
        ],
    ) {
        queries.extend([
            "go func",
            "goroutine",
            "context.With",
            "Close",
            "Wait",
            "Start",
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
        ]);
    }
    queries
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}
