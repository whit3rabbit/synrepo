use std::fs;

use crate::structure::graph::{GraphReader, SymbolNode, Visibility};
use crate::substrate::DiscoveryRoot;

use super::line_count::recommended_follow_up;
use super::util::{
    groups_for, is_test_file_path, physical_line_count, root_path_map, symbol_counts, PathMatcher,
};
use super::{
    MissingPublicDocSymbol, RefactorSuggestionCandidate, RefactorSuggestionCriteria,
    RefactorSuggestionMode, RefactorSuggestionOptions, RefactorSuggestionReport,
    DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT, METRIC_MISSING_PUBLIC_DOCS, SOURCE_STORE,
};

pub(crate) fn collect(
    reader: &dyn GraphReader,
    roots: &[DiscoveryRoot],
    options: &RefactorSuggestionOptions,
) -> crate::Result<RefactorSuggestionReport> {
    let matcher = PathMatcher::new(options.path_filter.as_deref())?;
    let root_paths = root_path_map(roots);
    let mut candidates = Vec::new();

    for (path, file_id) in reader.all_file_paths()? {
        if is_test_file_path(&path) || !matcher.matches(&path) {
            continue;
        }
        let Some(file) = reader.get_file(file_id)? else {
            continue;
        };
        if !supports_doc_comments(file.language.as_deref()) {
            continue;
        }
        let Some(root_path) = root_paths.get(&file.root_id) else {
            continue;
        };
        let absolute = root_path.join(&file.path);
        let Ok(bytes) = fs::read(&absolute) else {
            continue;
        };
        let line_count = physical_line_count(&bytes);
        let symbols = reader.symbols_for_file(file.id)?;
        let mut missing_docs = missing_public_docs(&symbols);
        if missing_docs.is_empty() {
            continue;
        }
        missing_docs.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
        let missing_public_doc_count = missing_docs.len();
        let missing_public_docs_omitted =
            missing_public_doc_count.saturating_sub(DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT);
        missing_docs.truncate(DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT);
        let symbol_counts = symbol_counts(&symbols);
        let tags = missing_docs_tags(missing_public_doc_count);
        candidates.push(RefactorSuggestionCandidate {
            path: file.path,
            file_id: file.id,
            language: file.language,
            line_count,
            size_bytes: file.size_bytes,
            symbol_counts,
            missing_public_doc_count,
            missing_public_docs: missing_docs,
            missing_public_docs_omitted,
            suggestion: missing_docs_suggestion(missing_public_doc_count),
            recommended_follow_up: recommended_follow_up(&path),
            modularity_tags: tags,
        });
    }

    candidates.sort_by(|a, b| {
        b.missing_public_doc_count
            .cmp(&a.missing_public_doc_count)
            .then_with(|| a.path.cmp(&b.path))
    });
    let candidate_count = candidates.len();
    let groups = groups_for(&candidates);
    let limit = options.limit;
    if candidates.len() > limit {
        candidates.truncate(limit);
    }
    let omitted_count = candidate_count.saturating_sub(candidates.len());
    Ok(RefactorSuggestionReport {
        source_store: SOURCE_STORE,
        mode: RefactorSuggestionMode::MissingDocs,
        metric: METRIC_MISSING_PUBLIC_DOCS,
        threshold: options.min_lines,
        criteria: RefactorSuggestionCriteria::missing_docs(options.min_lines),
        candidate_count,
        omitted_count,
        groups,
        candidates,
    })
}

fn supports_doc_comments(language: Option<&str>) -> bool {
    matches!(
        language,
        Some(
            "rust"
                | "python"
                | "typescript"
                | "tsx"
                | "go"
                | "javascript"
                | "java"
                | "kotlin"
                | "csharp"
                | "php"
                | "ruby"
                | "swift"
                | "c"
                | "cpp"
                | "dart"
        )
    )
}

fn missing_public_docs(symbols: &[SymbolNode]) -> Vec<MissingPublicDocSymbol> {
    symbols
        .iter()
        .filter(|symbol| symbol.visibility == Visibility::Public && symbol.doc_comment.is_none())
        .map(|symbol| MissingPublicDocSymbol {
            symbol_id: symbol.id,
            qualified_name: symbol.qualified_name.clone(),
            display_name: symbol.display_name.clone(),
            kind: symbol.kind,
            signature: symbol.signature.clone(),
        })
        .collect()
}

fn missing_docs_tags(count: usize) -> Vec<String> {
    let mut tags = vec!["missing_public_docs".to_string()];
    if count >= DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT {
        tags.push("many_missing_public_docs".to_string());
    }
    tags
}

fn missing_docs_suggestion(count: usize) -> String {
    if count == 1 {
        "Add an AST-recognized doc comment to the public symbol before expanding the API."
            .to_string()
    } else {
        "Add AST-recognized doc comments to the public symbols before expanding the API."
            .to_string()
    }
}
