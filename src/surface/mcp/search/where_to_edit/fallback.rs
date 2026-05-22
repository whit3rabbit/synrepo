pub(super) fn fallback_queries(task: &str) -> Vec<String> {
    crate::surface::query_terms::fallback_queries(task)
}

#[cfg(test)]
mod tests {
    use super::fallback_queries;

    #[test]
    fn fallback_queries_include_snake_case_plural_forms() {
        let queries = fallback_queries("agent hook routing with context metrics");

        assert!(queries.iter().any(|q| q == "agent_hooks"));
        assert!(queries.iter().any(|q| q == "context_metrics"));
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
