use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, SymbolNodeId};
use crate::structure::graph::SymbolKind;

/// Default physical-line threshold. Candidates must be greater than this.
pub const DEFAULT_MIN_LINES: usize = 300;
/// Default maximum candidates returned to callers.
pub const DEFAULT_LIMIT: usize = 20;
/// Default maximum missing public-doc symbols shown per candidate.
pub const DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT: usize = 5;
/// Stable metric label used in JSON responses.
pub const METRIC_PHYSICAL_LINES: &str = "physical_lines";
/// Stable metric label for missing public documentation responses.
pub const METRIC_MISSING_PUBLIC_DOCS: &str = "missing_public_docs";
/// Source-store label for suggestion output.
pub const SOURCE_STORE: &str = "graph+filesystem";

/// Suggestion mode.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactorSuggestionMode {
    /// Existing large-file line-count suggestions.
    #[default]
    LineCount,
    /// Public symbols that lack parser-extracted documentation.
    MissingDocs,
}

impl RefactorSuggestionMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LineCount => "line count",
            Self::MissingDocs => "missing docs",
        }
    }

    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::LineCount => Self::MissingDocs,
            Self::MissingDocs => Self::LineCount,
        }
    }
}

/// Mode-specific criteria used to produce a suggestion report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefactorSuggestionCriteria {
    /// Physical-line threshold supplied to the report.
    pub line_count_threshold: usize,
    /// Whether the physical-line threshold was used for candidate eligibility.
    pub line_count_threshold_applied: bool,
    /// Symbol visibility included by the active mode, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<&'static str>,
    /// Documentation source inspected by the active mode, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_source: Option<&'static str>,
}

impl RefactorSuggestionCriteria {
    pub(crate) fn line_count(threshold: usize) -> Self {
        Self {
            line_count_threshold: threshold,
            line_count_threshold_applied: true,
            visibility: None,
            doc_source: None,
        }
    }

    pub(crate) fn missing_docs(threshold: usize) -> Self {
        Self {
            line_count_threshold: threshold,
            line_count_threshold_applied: false,
            visibility: Some("public"),
            doc_source: Some("ast_doc_comment"),
        }
    }
}

/// Options controlling refactor-suggestion collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefactorSuggestionOptions {
    /// Suggestion mode.
    pub mode: RefactorSuggestionMode,
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
            mode: RefactorSuggestionMode::LineCount,
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
    /// Suggestion mode used to produce this report.
    pub mode: RefactorSuggestionMode,
    /// Metric label for the active mode.
    pub metric: &'static str,
    /// Physical-line threshold used for line-count eligibility.
    pub threshold: usize,
    /// Mode-specific criteria used for eligibility.
    pub criteria: RefactorSuggestionCriteria,
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
    /// Largest missing public-doc count in this language group.
    pub max_missing_public_doc_count: usize,
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

/// One public symbol missing parser-extracted documentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MissingPublicDocSymbol {
    /// Stable graph symbol ID.
    pub symbol_id: SymbolNodeId,
    /// Fully qualified symbol name within its file.
    pub qualified_name: String,
    /// Short display name.
    pub display_name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// One-line signature, if extracted.
    pub signature: Option<String>,
}

/// One refactor suggestion candidate.
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
    /// Public symbols in this file without parser-extracted documentation.
    pub missing_public_doc_count: usize,
    /// Bounded preview of missing public-doc symbols.
    pub missing_public_docs: Vec<MissingPublicDocSymbol>,
    /// Missing public-doc symbols omitted from the preview.
    pub missing_public_docs_omitted: usize,
    /// Deterministic classification tags used to explain the suggestion.
    pub modularity_tags: Vec<String>,
    /// Short deterministic suggestion for an LLM or operator to refine.
    pub suggestion: String,
    /// Suggested follow-up MCP tools for deeper analysis.
    pub recommended_follow_up: Vec<String>,
}
