//! Deterministic refactor-suggestion candidates for large source files.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use globset::Glob;
use serde::Serialize;

use crate::config::Config;
use crate::core::ids::FileNodeId;
use crate::store::sqlite::SqliteGraphStore;
use crate::structure::graph::Visibility;
use crate::substrate::{discover_roots, DiscoveryRoot};
use crate::surface::card::compiler::GraphCardCompiler;

#[cfg(test)]
mod tests;

/// Default physical-line threshold. Candidates must be greater than this.
pub const DEFAULT_MIN_LINES: usize = 300;
/// Default maximum candidates returned to callers.
pub const DEFAULT_LIMIT: usize = 20;
/// Stable metric label used in JSON responses.
pub const METRIC_PHYSICAL_LINES: &str = "physical_lines";
/// Source-store label for suggestion output.
pub const SOURCE_STORE: &str = "graph+filesystem";

/// Options controlling refactor-suggestion collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefactorSuggestionOptions {
    /// Physical-line threshold. Files must be greater than this value.
    pub min_lines: usize,
    /// Maximum candidates to return after deterministic sorting.
    pub limit: usize,
    /// Optional path prefix or glob filter.
    pub path_filter: Option<String>,
}

impl Default for RefactorSuggestionOptions {
    fn default() -> Self {
        Self {
            min_lines: DEFAULT_MIN_LINES,
            limit: DEFAULT_LIMIT,
            path_filter: None,
        }
    }
}

/// Complete refactor-suggestion response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefactorSuggestionReport {
    /// Source-store label, always `graph+filesystem`.
    pub source_store: &'static str,
    /// Metric label, always `physical_lines`.
    pub metric: &'static str,
    /// Physical-line threshold used for eligibility.
    pub threshold: usize,
    /// Number of matching candidates before limit truncation.
    pub candidate_count: usize,
    /// Number of matching candidates omitted due to the limit.
    pub omitted_count: usize,
    /// Language-level grouping over all matching candidates.
    pub groups: Vec<RefactorSuggestionGroup>,
    /// Returned candidates after sorting and limiting.
    pub candidates: Vec<RefactorSuggestionCandidate>,
}

/// Language-level candidate summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefactorSuggestionGroup {
    /// Language label, or `unknown`.
    pub language: String,
    /// Number of matching candidates in this language group.
    pub count: usize,
    /// Largest physical-line count in this language group.
    pub max_line_count: usize,
}

/// Symbol-count summary for a candidate file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefactorSymbolCounts {
    /// Active symbols currently owned by the file.
    pub total: usize,
    /// Active public symbols currently owned by the file.
    pub public: usize,
    /// Active crate-visible or protected symbols currently owned by the file.
    pub restricted: usize,
    /// Active private symbols currently owned by the file.
    pub private: usize,
}

/// One large-file refactor suggestion candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefactorSuggestionCandidate {
    /// File path relative to its discovery root.
    pub path: String,
    /// Stable graph file ID.
    pub file_id: FileNodeId,
    /// Detected language label from the graph.
    pub language: Option<String>,
    /// Physical line count from the filesystem.
    pub line_count: usize,
    /// File size in bytes from the graph.
    pub size_bytes: u64,
    /// Active symbol-count summary.
    pub symbol_counts: RefactorSymbolCounts,
    /// Deterministic classification tags used to explain the suggestion.
    pub modularity_tags: Vec<String>,
    /// Short deterministic suggestion for an LLM or operator to refine.
    pub suggestion: String,
    /// Suggested follow-up MCP tools for deeper analysis.
    pub recommended_follow_up: Vec<String>,
}

/// Collect refactor suggestions for a repository by opening its graph store.
pub fn collect_refactor_suggestions_for_repo(
    repo_root: &Path,
    options: RefactorSuggestionOptions,
) -> crate::Result<RefactorSuggestionReport> {
    let config = Config::load(repo_root)?;
    let graph_dir = Config::synrepo_dir(repo_root).join("graph");
    let graph = SqliteGraphStore::open_existing(&graph_dir)?;
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo_root.to_path_buf()))
        .with_config(config.clone());
    let roots = discover_roots(repo_root, &config);
    collect_refactor_suggestions(&compiler, &roots, options)
}

/// Collect refactor suggestions from an existing graph-card compiler.
pub fn collect_refactor_suggestions(
    compiler: &GraphCardCompiler,
    roots: &[DiscoveryRoot],
    options: RefactorSuggestionOptions,
) -> crate::Result<RefactorSuggestionReport> {
    compiler.with_reader(|reader| {
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
            metric: METRIC_PHYSICAL_LINES,
            threshold: options.min_lines,
            candidate_count,
            omitted_count,
            groups,
            candidates,
        })
    })
}

fn root_path_map(roots: &[DiscoveryRoot]) -> BTreeMap<String, PathBuf> {
    roots
        .iter()
        .map(|root| (root.discriminant.clone(), root.absolute_path.clone()))
        .collect()
}

fn physical_line_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    bytes.iter().filter(|byte| **byte == b'\n').count() + usize::from(!bytes.ends_with(b"\n"))
}

fn symbol_counts(symbols: &[crate::structure::graph::SymbolNode]) -> RefactorSymbolCounts {
    let mut counts = RefactorSymbolCounts {
        total: symbols.len(),
        public: 0,
        restricted: 0,
        private: 0,
    };
    for sym in symbols {
        match sym.visibility {
            Visibility::Public => counts.public += 1,
            Visibility::Crate | Visibility::Protected => counts.restricted += 1,
            Visibility::Private => counts.private += 1,
            Visibility::Unknown => {}
        }
    }
    counts
}

fn modularity_tags(
    path: &str,
    language: Option<&str>,
    line_count: usize,
    source: &str,
    symbols: &RefactorSymbolCounts,
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

fn recommended_follow_up(path: &str) -> Vec<String> {
    vec![
        format!("synrepo_card target={path} budget=normal"),
        format!("synrepo_minimum_context target={path} budget=normal"),
    ]
}

fn groups_for(candidates: &[RefactorSuggestionCandidate]) -> Vec<RefactorSuggestionGroup> {
    let mut groups: BTreeMap<String, RefactorSuggestionGroup> = BTreeMap::new();
    for candidate in candidates {
        let language = candidate
            .language
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let group = groups
            .entry(language.clone())
            .or_insert(RefactorSuggestionGroup {
                language,
                count: 0,
                max_line_count: 0,
            });
        group.count += 1;
        group.max_line_count = group.max_line_count.max(candidate.line_count);
    }
    groups.into_values().collect()
}

fn is_test_file_path(path: &str) -> bool {
    let Some(name) = file_name(path) else {
        return false;
    };
    path.split('/')
        .any(|part| matches!(part, "tests" | "__tests__"))
        || name == "tests.rs"
        || name.starts_with("test_")
        || name.contains("_test.")
        || name.contains("_tests.")
        || name.contains(".test.")
        || name.contains(".spec.")
}

fn file_name(path: &str) -> Option<&str> {
    Path::new(path).file_name().and_then(|name| name.to_str())
}

fn file_stem(path: &str) -> Option<&str> {
    Path::new(path).file_stem().and_then(|stem| stem.to_str())
}

struct PathMatcher {
    filter: Option<PathFilter>,
}

enum PathFilter {
    Prefix(String),
    Glob(globset::GlobMatcher),
}

impl PathMatcher {
    fn new(filter: Option<&str>) -> crate::Result<Self> {
        let Some(filter) = filter.filter(|value| !value.trim().is_empty()) else {
            return Ok(Self { filter: None });
        };
        if contains_glob_chars(filter) {
            let glob = Glob::new(filter).map_err(|err| {
                crate::Error::Config(format!("invalid path filter glob `{filter}`: {err}"))
            })?;
            Ok(Self {
                filter: Some(PathFilter::Glob(glob.compile_matcher())),
            })
        } else {
            Ok(Self {
                filter: Some(PathFilter::Prefix(filter.to_string())),
            })
        }
    }

    fn matches(&self, path: &str) -> bool {
        match &self.filter {
            None => true,
            Some(PathFilter::Prefix(prefix)) => path.starts_with(prefix),
            Some(PathFilter::Glob(glob)) => glob.is_match(path),
        }
    }
}

fn contains_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}
