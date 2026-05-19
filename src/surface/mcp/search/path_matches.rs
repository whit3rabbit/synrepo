use std::collections::HashSet;

use serde_json::{json, Value};

use crate::surface::mcp::SynrepoState;

const PRIMARY_ROOT_ID: &str = "primary";

pub(super) fn prepend_direct_path_matches(
    state: &SynrepoState,
    query: &str,
    limit: usize,
    items: &mut Vec<Value>,
) -> anyhow::Result<()> {
    let Some(path) = normalized_path_query(query) else {
        return Ok(());
    };

    let mut direct = Vec::new();
    for file in crate::substrate::discover(&state.repo_root, &state.config)? {
        if file.relative_path == path {
            direct.push(json!({
                "path": file.relative_path,
                "root_id": file.root_discriminant,
                "is_primary_root": file.root_discriminant == PRIMARY_ROOT_ID,
                "file_id": Value::Null,
                "line": Value::Null,
                "content": Value::Null,
                "source": "path",
                "fusion_score": Value::Null,
                "semantic_score": Value::Null,
            }));
        }
    }
    if direct.is_empty() {
        return Ok(());
    }

    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for item in direct.iter().chain(items.iter()) {
        let path = item.get("path").and_then(Value::as_str).unwrap_or_default();
        let root_id = item
            .get("root_id")
            .and_then(Value::as_str)
            .unwrap_or(PRIMARY_ROOT_ID);
        if seen.insert(format!("{root_id}\0{path}")) {
            merged.push(item.clone());
        }
        if merged.len() >= limit {
            break;
        }
    }
    *items = merged;
    Ok(())
}

fn normalized_path_query(query: &str) -> Option<String> {
    let trimmed = query
        .trim()
        .trim_matches(|ch: char| matches!(ch, '`' | '"' | '\'' | '<' | '>'));
    if trimmed.is_empty() || !trimmed.contains('/') {
        return None;
    }
    if trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    if trimmed.contains('*') || trimmed.contains('?') || trimmed.contains('|') {
        return None;
    }
    Some(trimmed.trim_start_matches("./").replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::normalized_path_query;

    #[test]
    fn normalizes_exact_path_queries_only() {
        assert_eq!(
            normalized_path_query("`./docs/SECURITY.md`").as_deref(),
            Some("docs/SECURITY.md")
        );
        assert_eq!(normalized_path_query("memory/resource leaks"), None);
    }
}
