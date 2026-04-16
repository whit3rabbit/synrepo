use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use crate::overlay::{ConfidenceTier, CrossLinkFreshness, OverlayEdgeKind};
use crate::structure::graph::{Epistemic, SymbolKind};

use super::{FileGitIntelligence, SourceStore, SymbolLastChange};

/// A reference to a caller or callee in a SymbolCard.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolRef {
    /// Node ID of the referenced symbol.
    pub id: SymbolNodeId,
    /// Qualified name for display.
    pub qualified_name: String,
    /// File path and line for display.
    pub location: String,
}

/// A reference to a file in a FileCard or similar.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileRef {
    /// Node ID of the referenced file.
    pub id: FileNodeId,
    /// Path relative to the repo root.
    pub path: String,
}

/// SymbolCard — answers "what is this function/class, how is it connected?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolCard {
    /// The symbol this card describes.
    pub symbol: SymbolNodeId,
    /// Display name (short form).
    pub name: String,
    /// Fully qualified name within its file.
    pub qualified_name: String,
    /// File and line where defined.
    pub defined_at: String,
    /// One-line signature.
    pub signature: Option<String>,
    /// Doc comment, truncated for `tiny` budget.
    pub doc_comment: Option<String>,
    /// Callers (symbols that call this one). Truncated per budget.
    pub callers: Vec<SymbolRef>,
    /// Callees (symbols this one calls). Truncated per budget.
    pub callees: Vec<SymbolRef>,
    /// Test symbols that exercise this one. Empty for `tiny`.
    pub tests_touching: Vec<SymbolRef>,
    /// Most recent commit touching this symbol's containing file (V1
    /// granularity: `File`). Absent at `Tiny` budget; revision + author +
    /// timestamp at `Normal`; adds the folded summary at `Deep`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<SymbolLastChange>,
    /// Drift score and flag, if any.
    pub drift_flag: Option<String>,
    /// Full source body, only populated for `Deep` budget.
    pub source_body: Option<String>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Every field in this card came from the graph; synthesis commentary
    /// is a separate field below if present.
    pub source_store: SourceStore,
    /// Epistemic origin of the primary fields.
    pub epistemic: Epistemic,
    /// Optional LLM-authored commentary from the overlay, clearly marked.
    /// Only populated if the card was requested at `Deep` budget and
    /// commentary exists in the overlay.
    pub overlay_commentary: Option<OverlayCommentary>,
    /// Flat commentary state label exposed to MCP callers so they can
    /// distinguish `budget_withheld` (Tiny/Normal) from `missing`, `fresh`,
    /// `stale`, `invalid`, or `unsupported` (Deep). Parallel to
    /// `overlay_commentary` so callers can branch on the state without
    /// deserializing the nested object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commentary_state: Option<String>,
    /// Proposed cross-links authored by the synthesis layer with evidence verification.
    /// Only populated at Deep budget.
    pub proposed_links: Option<Vec<ProposedLink>>,
    /// State of the proposed links (e.g., "budget_withheld", "fresh", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links_state: Option<String>,
}

/// LLM-authored commentary layered on top of a structural card.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayCommentary {
    /// The commentary text.
    pub text: String,
    /// Freshness state of the commentary.
    pub freshness: Freshness,
    /// Source store is always `Overlay` for commentary.
    pub source_store: SourceStore,
}

/// Freshness state of an overlay entry.
///
/// Mirrors the five spec states from `FreshnessState` in `src/overlay/mod.rs`:
/// `Fresh`, `Stale`, `Invalid`, `Missing`, `Unsupported`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// The commentary is current with the source it describes.
    Fresh,
    /// The source has changed since the commentary was produced.
    Stale,
    /// Entry is present but missing one or more required provenance fields.
    Invalid,
    /// No commentary exists for this target yet.
    Missing,
    /// The node kind has no commentary pipeline defined.
    Unsupported,
}

impl From<crate::overlay::FreshnessState> for Freshness {
    fn from(state: crate::overlay::FreshnessState) -> Self {
        match state {
            crate::overlay::FreshnessState::Fresh => Self::Fresh,
            crate::overlay::FreshnessState::Stale => Self::Stale,
            crate::overlay::FreshnessState::Invalid => Self::Invalid,
            crate::overlay::FreshnessState::Missing => Self::Missing,
            crate::overlay::FreshnessState::Unsupported => Self::Unsupported,
        }
    }
}

/// A proposed cross-link surfaced on a structural card.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposedLink {
    /// Node ID of the source.
    pub source: NodeId,
    /// Node ID of the target.
    pub target: NodeId,
    /// Kind of edge proposed.
    pub kind: OverlayEdgeKind,
    /// Confidence tier.
    pub tier: ConfidenceTier,
    /// Freshness of this proposed link compared to the current file content.
    pub freshness: CrossLinkFreshness,
    /// Number of spans cited as evidence.
    pub span_count: usize,
}

/// FileCard — answers "what's in this file, what depends on it?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileCard {
    /// The file this card describes.
    pub file: FileNodeId,
    /// Path relative to the repo root.
    pub path: String,
    /// Top-level symbols in the file.
    pub symbols: Vec<SymbolRef>,
    /// Files that import this one.
    pub imported_by: Vec<FileRef>,
    /// Files this one imports.
    pub imports: Vec<FileRef>,
    /// Files that co-change with this one without an import edge (hidden coupling).
    pub co_changes: Vec<FileRef>,
    /// Git-derived recent change context for this file, if available.
    pub git_intelligence: Option<FileGitIntelligence>,
    /// Drift flag summary across edges incident to this file.
    pub drift_flag: Option<String>,
    /// Approximate token count.
    pub approx_tokens: usize,
    /// Source store.
    pub source_store: SourceStore,
    /// Proposed cross-links authored by the synthesis layer.
    pub proposed_links: Option<Vec<ProposedLink>>,
    /// State of the proposed links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links_state: Option<String>,
}

/// ModuleCard — answers "what lives in this directory/module?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleCard {
    /// Path of the directory or module root (e.g. `src/auth/`).
    pub path: String,
    /// Top-level files directly inside this directory (not in subdirectories).
    pub files: Vec<FileRef>,
    /// Immediate subdirectory paths; agents can request ModuleCards for each.
    pub nested_modules: Vec<String>,
    /// Top-level public symbols visible across module boundaries.
    /// Empty at `Tiny` budget; populated at `Normal` and `Deep`.
    pub public_symbols: Vec<SymbolRef>,
    /// Total count of public symbols across all direct files (always populated).
    pub total_symbol_count: usize,
    /// Approximate token count.
    pub approx_tokens: usize,
    /// Source store.
    pub source_store: SourceStore,
}

/// One exported symbol in a `PublicAPICard`.
///
/// Visibility is inferred from `signature`: if it starts with `pub`, the
/// symbol is considered exported. This heuristic works for Rust (`pub fn`,
/// `pub struct`, `pub(crate)`, etc.). For Python, TypeScript, and Go, where
/// visibility is not expressed as a `pub` keyword, `public_symbols` will be
/// empty in v1.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicAPIEntry {
    /// Stable node ID of the symbol.
    pub id: SymbolNodeId,
    /// Short display name.
    pub name: String,
    /// Symbol kind (function, struct, trait, etc.).
    pub kind: SymbolKind,
    /// Full declaration prefix, e.g. `pub fn parse(input: &str) -> Result<…>`.
    /// The `pub` prefix is the visibility signal; callers may inspect it directly.
    pub signature: String,
    /// `"path:byte_offset"` for IDE navigation.
    pub location: String,
    /// Most recent change for this symbol's containing file.
    /// Absent at `Tiny`; present at `Normal` and `Deep`.
    /// At `Deep`, includes a human-readable summary string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<SymbolLastChange>,
}

/// `PublicAPICard` — answers "what does this module/crate expose?"
///
/// Surfaces the exported API of a directory: public symbols with kinds and
/// signatures, public entry points (the subset also detected as execution
/// entry points), and (at `Deep` budget) symbols whose containing file was
/// last touched within 30 days.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicAPICard {
    /// Directory path this card describes (normalised with trailing `/`).
    pub path: String,
    /// Public symbols from direct-child files.
    /// Empty at `Tiny`; populated at `Normal` and `Deep`.
    pub public_symbols: Vec<PublicAPIEntry>,
    /// Count of all public symbols across direct-child files (always present).
    pub public_symbol_count: usize,
    /// Subset of `public_symbols` also classified as execution entry points.
    /// Empty at `Tiny`; populated at `Normal` and `Deep`.
    pub public_entry_points: Vec<PublicAPIEntry>,
    /// Public symbols whose containing file was last touched within 30 days.
    /// Only populated at `Deep` budget; omitted from JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_api_changes: Vec<PublicAPIEntry>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` for public-API cards).
    pub source_store: SourceStore,
}

/// Classification of an execution entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointKind {
    /// Binary entry point: a `main` function in `src/main.rs` or `src/bin/`.
    Binary,
    /// CLI command handler in a file whose path contains `cli`, `command`, or `cmd`.
    CliCommand,
    /// HTTP route handler: name starts with `handle_`, `serve_`, or `route_`,
    /// or the file path contains `handler`, `route`, or `router`.
    HttpHandler,
    /// Public item at a library root (`src/lib.rs` or a `mod.rs` boundary).
    LibRoot,
}

/// A single detected execution entry point.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPoint {
    /// Stable node ID of the entry-point symbol.
    pub symbol: SymbolNodeId,
    /// Fully qualified name within its file.
    pub qualified_name: String,
    /// File path and byte offset (e.g. `src/main.rs:0`).
    pub location: String,
    /// Classification of this entry point.
    pub kind: EntryPointKind,
    /// Number of unique callers in the graph. `None` at `Tiny` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_count: Option<usize>,
    /// Doc comment truncated to 80 characters. `None` at `Tiny` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// One-line signature. `None` below `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// EntryPointCard — answers "where does execution start in this scope?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPointCard {
    /// Optional path-prefix scope that was requested (`None` = whole repo).
    pub scope: Option<String>,
    /// Detected entry points, sorted by kind then file path, capped at 20.
    pub entry_points: Vec<EntryPoint>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` for entry-point cards).
    pub source_store: SourceStore,
}

/// A single edge in a call path from entry point to target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallPathEdge {
    /// Source symbol (caller).
    pub from: SymbolRef,
    /// Target symbol (callee).
    pub to: SymbolRef,
    /// Kind of edge (always "Calls" for v1).
    pub edge_kind: String,
    /// Whether this path was truncated due to depth limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

/// A single call path from an entry point to the target symbol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallPath {
    /// The entry point symbol where this path starts.
    pub entry_point: SymbolRef,
    /// The target symbol at the end of this path.
    pub target: SymbolRef,
    /// Ordered list of edges from entry point to target.
    pub edges: Vec<CallPathEdge>,
    /// Number of additional paths omitted due to deduplication cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths_omitted: Option<usize>,
}

/// CallPathCard — answers "how do I reach this function from entry points?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallPathCard {
    /// The target symbol this card traces paths to.
    pub target: SymbolRef,
    /// All discovered call paths from entry points to the target.
    pub paths: Vec<CallPath>,
    /// Total count of omitted paths across all (entry_point, target) pairs.
    pub paths_omitted: usize,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` for call-path cards).
    pub source_store: SourceStore,
}

/// How a test was associated with a source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestAssociation {
    /// Matched by SymbolKind::Test.
    SymbolKind,
    /// Matched by file path convention.
    PathConvention,
    /// Matched by both signals.
    Both,
}

/// A single test entry discovered for a source file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestEntry {
    /// Node ID of the test symbol.
    pub symbol_id: SymbolNodeId,
    /// Fully qualified name of the test.
    pub qualified_name: String,
    /// Path of the file containing this test (repo-relative).
    pub file_path: String,
    /// The associated source file path.
    pub source_file: String,
    /// How this test was associated with the source file.
    pub association: TestAssociation,
    /// One-line signature. Populated only at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Doc comment, truncated to 120 chars. Populated only at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// Production symbols called by this test. Populated only at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub covers: Option<Vec<SymbolNodeId>>,
}

/// TestSurfaceCard — answers "what tests cover this code?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestSurfaceCard {
    /// The scope this card was compiled for (file path or directory).
    pub scope: String,
    /// Discovered test entries grouped by source file.
    pub tests: Vec<TestEntry>,
    /// Total count of test files discovered.
    pub test_file_count: usize,
    /// Total count of test symbols discovered.
    pub test_symbol_count: usize,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` for test-surface cards).
    pub source_store: SourceStore,
}

/// Risk level classification for change risk assessment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Low risk: score < 0.4
    Low,
    /// Medium risk: score >= 0.4
    Medium,
    /// High risk: score >= 0.6
    High,
    /// Critical risk: score >= 0.8
    Critical,
}

impl RiskLevel {
    /// Derive risk level from a composite score (0-1).
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::Critical
        } else if score >= 0.6 {
            Self::High
        } else if score >= 0.4 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// A single contributing factor to the risk score.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Signal type identifier.
    pub signal: String,
    /// Raw value before normalization.
    pub raw_value: f64,
    /// Normalized value (0-1 scale).
    pub normalized_value: f64,
    /// Human-readable description of this factor.
    pub description: String,
}

/// ChangeRiskCard — answers "what is the risk of changing this symbol or file?"
///
/// Aggregates drift score, co-change relationships, and git hotspot data
/// into a risk assessment computed on-demand from graph signals.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeRiskCard {
    /// Target this card assesses (symbol or file).
    pub target: NodeId,
    /// Display name of the target.
    pub target_name: String,
    /// Target kind ("symbol" or "file").
    pub target_kind: String,
    /// Overall risk level.
    pub risk_level: RiskLevel,
    /// Composite risk score (0-1 weighted sum).
    pub risk_score: f64,
    /// Drift score from structural fingerprint changes (0-1).
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_score: Option<f64>,
    /// Count of co-change partners, normalized to 0-1.
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_change_partner_count: Option<f64>,
    /// Recent touch frequency score from git intelligence (0-1).
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotspot_score: Option<f64>,
    /// Contributing risk factors. Populated at `Normal` and `Deep` budgets.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub risk_factors: Vec<RiskFactor>,
    /// Count of outgoing edges with drift scores at the current revision.
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_edge_count: Option<usize>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` — ChangeRiskCard uses graph signals only).
    pub source_store: SourceStore,
}
