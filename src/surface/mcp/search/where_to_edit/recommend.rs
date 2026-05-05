use std::collections::HashSet;

const MAX_RECOMMENDED_NEXT_QUERIES: usize = 10;

pub(super) fn recommended_next_queries(task: &str, miss_reason: Option<&str>) -> Vec<String> {
    if miss_reason.is_none() {
        return Vec::new();
    }

    let lower = task.to_ascii_lowercase();
    let mut queries = Vec::new();
    let mut seen = HashSet::new();
    add_code_shaped_tokens(&mut queries, &mut seen, task);

    if contains_any(&lower, &["mcp", "tool", "registration"]) {
        add_recommendation(&mut queries, &mut seen, "name = \"synrepo_");
        add_recommendation(&mut queries, &mut seen, "registered_tool_names");
    }
    if contains_any(&lower, &["parameter", "param", "validation", "budget"]) {
        add_recommendation(&mut queries, &mut seen, "parse_budget");
        add_recommendation(&mut queries, &mut seen, "budget_tokens");
        add_recommendation(&mut queries, &mut seen, "INVALID_PARAMETER");
    }
    if contains_any(
        &lower,
        &["mutability", "mutation", "edit", "write", "gating"],
    ) {
        add_recommendation(&mut queries, &mut seen, "allow-source-edits");
        add_recommendation(&mut queries, &mut seen, "allow-overlay-writes");
        add_recommendation(&mut queries, &mut seen, "synrepo_refresh_commentary");
        add_recommendation(&mut queries, &mut seen, "synrepo_note_add");
        add_recommendation(&mut queries, &mut seen, "apply_anchor_edits");
    }
    if contains_any(&lower, &["error", "response"]) {
        add_recommendation(&mut queries, &mut seen, "response_has_error");
        add_recommendation(&mut queries, &mut seen, "render_error");
        add_recommendation(&mut queries, &mut seen, "error.code");
    }
    if contains_any(&lower, &["resource", "resources"]) {
        add_recommendation(&mut queries, &mut seen, "read_resource");
        add_recommendation(&mut queries, &mut seen, "read_resource_blocking");
    }
    if contains_any(&lower, &["agent hook", "agent hooks", "nudge"]) {
        add_recommendation(&mut queries, &mut seen, "agent_hooks");
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
            || token.starts_with("synrepo")
            || has_mixed_case(token)
        {
            add_recommendation(queries, seen, token);
            if let Some(stripped) = token.strip_prefix("--") {
                add_recommendation(queries, seen, stripped);
            }
        }
    }
}

fn has_mixed_case(token: &str) -> bool {
    token.chars().any(|ch| ch.is_ascii_lowercase())
        && token.chars().any(|ch| ch.is_ascii_uppercase())
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn add_recommendation(queries: &mut Vec<String>, seen: &mut HashSet<String>, query: &str) {
    if queries.len() >= MAX_RECOMMENDED_NEXT_QUERIES || query.len() < 3 {
        return;
    }
    if seen.insert(query.to_string()) {
        queries.push(query.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::recommended_next_queries;

    #[test]
    fn converts_mcp_review_language_to_exact_probes() {
        let queries = recommended_next_queries(
            "review MCP mutability and parameter validation",
            Some("no_index_matches"),
        );

        assert!(queries.iter().any(|q| q == "name = \"synrepo_"));
        assert!(queries.iter().any(|q| q == "allow-source-edits"));
        assert!(queries.iter().any(|q| q == "parse_budget"));
    }

    #[test]
    fn extracts_exact_code_shaped_tokens() {
        let queries = recommended_next_queries(
            "inspect synrepo_refresh_commentary and --allow-overlay-writes",
            Some("no_index_matches"),
        );

        assert!(queries.iter().any(|q| q == "synrepo_refresh_commentary"));
        assert!(queries.iter().any(|q| q == "--allow-overlay-writes"));
        assert!(queries.iter().any(|q| q == "allow-overlay-writes"));
    }
}
