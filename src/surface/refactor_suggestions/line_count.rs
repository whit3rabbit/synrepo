use std::fs;

use crate::structure::graph::GraphReader;
use crate::substrate::DiscoveryRoot;

use super::util::{
    file_name, file_stem, groups_for, is_test_file_path, physical_line_count, root_path_map,
    symbol_counts, PathMatcher,
};
use super::{
    RefactorSuggestionCandidate, RefactorSuggestionCriteria, RefactorSuggestionMode,
    RefactorSuggestionOptions, RefactorSuggestionReport, METRIC_PHYSICAL_LINES, SOURCE_STORE,
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
        if file.language.is_none() {
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
        if line_count <= options.min_lines {
            continue;
        }

        let source = String::from_utf8_lossy(&bytes);
        let symbols = reader.symbols_for_file(file.id)?;
        let symbol_counts = symbol_counts(&symbols);
        let tags = modularity_tags(
            &file.path,
            file.language.as_deref(),
            line_count,
            &source,
            &symbol_counts,
        );
        candidates.push(RefactorSuggestionCandidate {
            path: file.path,
            file_id: file.id,
            language: file.language,
            line_count,
            size_bytes: file.size_bytes,
            symbol_counts,
            missing_public_doc_count: 0,
            missing_public_docs: Vec::new(),
            missing_public_docs_omitted: 0,
            suggestion: suggestion_for(&tags),
            recommended_follow_up: recommended_follow_up(&path),
            modularity_tags: tags,
        });
    }

    candidates.sort_by(|a, b| {
        b.line_count
            .cmp(&a.line_count)
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
        mode: RefactorSuggestionMode::LineCount,
        metric: METRIC_PHYSICAL_LINES,
        threshold: options.min_lines,
        criteria: RefactorSuggestionCriteria::line_count(options.min_lines),
        candidate_count,
        omitted_count,
        groups,
        candidates,
    })
}

fn modularity_tags(
    path: &str,
    language: Option<&str>,
    line_count: usize,
    source: &str,
    symbols: &super::RefactorSymbolCounts,
) -> Vec<String> {
    let mut tags = vec!["large_file".to_string()];
    if line_count >= 400 {
        tags.push("over_repo_cap".to_string());
    }
    if symbols.total >= 10 {
        tags.push("many_symbols".to_string());
    }
    if matches!(language, Some("rust")) {
        if is_module_root(path) {
            tags.push("rust_module_root".to_string());
        }
        if declares_rust_modules(source) {
            tags.push("declares_modules".to_string());
        }
    }
    if matches!(language, Some("typescript" | "tsx" | "javascript")) {
        if source.contains("export ") {
            tags.push("exports_api".to_string());
        }
        if file_stem(path).is_some_and(|stem| stem == "index") {
            tags.push("module_barrel_or_root".to_string());
        }
    }
    if tags.len() == 1 {
        tags.push("extract_cohesive_units".to_string());
    }
    tags
}

fn is_module_root(path: &str) -> bool {
    matches!(file_name(path), Some("mod.rs" | "lib.rs" | "main.rs"))
}

fn declares_rust_modules(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("mod ") || trimmed.starts_with("pub mod ")
    })
}

fn suggestion_for(tags: &[String]) -> String {
    if tags.iter().any(|tag| tag == "rust_module_root") {
        return "Split this Rust module root into focused sibling modules and re-export the stable public surface.".to_string();
    }
    if tags.iter().any(|tag| tag == "declares_modules") {
        return "Move related item groups behind the existing module declarations instead of growing the root file.".to_string();
    }
    if tags.iter().any(|tag| tag == "many_symbols") {
        return "Group related symbols into focused files or modules, then keep current entrypoints as thin re-exports.".to_string();
    }
    "Review for cohesive sections that can move into focused modules while preserving current behavior.".to_string()
}

pub(crate) fn recommended_follow_up(path: &str) -> Vec<String> {
    vec![
        format!("synrepo_card target={path} budget=normal"),
        format!("synrepo_minimum_context target={path} budget=normal"),
    ]
}
